#!/usr/bin/env python3
"""How far a list moved on each repaint of the private display, from a
`steps` recording of scripts/ui-session (record_host --rows).

  testing/frame_steps.py FILE.rows [--from Y0] [--to Y1] [--most PX] [--csv OUT]

Each repaint's row sums (a column band's pixels added up, one number a row)
are matched against the repaint before it: the shift, up to --most pixels
either way, at which rows Y0 to Y1 differ least is how far the content
moved. It prints, over the repaints from the first move to the last, each
repaint's time since the one before and the shift, then a summary: the
repaints' intervals, how many moves came and how many repaints in between
showed no move (a frame the window missed), how many repaints went
uncaptured (the recording's own misses: the steps either side of one are
two repaints' worth), and the steps' sizes.

Standard library only, as the host's python3.
"""

import argparse
import statistics
import struct
import sys


def read(path):
    with open(path, "rb") as source:
        header = source.readline().decode().split()
        if len(header) != 5 or header[0] != "rows":
            raise SystemExit(f"{path}: not a steps recording")
        _, width, height, x0, x1 = header
        height = int(height)
        size = 8 + 4 * height
        frames = []
        while True:
            chunk = source.read(size)
            if len(chunk) < size:
                break
            (nanos,) = struct.unpack_from("<Q", chunk)
            sums = struct.unpack_from(f"<{height}I", chunk, 8)
            # The GL framebuffer's rows come bottom first.
            frames.append((nanos / 1e6, sums[::-1]))
    return int(width), height, int(x0), int(x1), frames


def mismatch(before, after, shift, y0, y1, limit):
    """How much rows y0..y1 of `after` differ from `before`'s moved by `shift`
    (content moving up the screen is a positive shift), per row."""
    total = 0
    count = 0
    for y in range(y0, y1):
        source = y + shift
        if 0 <= source < len(before):
            total += abs(after[y] - before[source])
            count += 1
            if total > limit * (y1 - y0):
                return float("inf")
    return total / count if count else float("inf")


def shift_between(before, after, y0, y1, most, guess):
    if before == after:
        return 0, 0.0
    best, best_score = 0, mismatch(before, after, 0, y0, y1, float("inf"))
    # Near the last step first, which prunes the rest.
    order = sorted(range(-most, most + 1), key=lambda shift: abs(shift - guess))
    for shift in order:
        if shift == 0:
            continue
        score = mismatch(before, after, shift, y0, y1, best_score)
        if score < best_score:
            best, best_score = shift, score
    return best, best_score


def main():
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument("file")
    parser.add_argument(
        "--from", dest="y0", type=int, help="first row of the list (default: 10%% down)"
    )
    parser.add_argument("--to", dest="y1", type=int, help="row after the list (default: 90%% down)")
    parser.add_argument("--most", type=int, default=120, help="largest step looked for, in px")
    parser.add_argument("--csv", help="write each repaint: ms, interval ms, shift px")
    parser.add_argument("--quiet", action="store_true", help="summary only")
    options = parser.parse_args()
    _, height, _, _, frames = read(options.file)
    if len(frames) < 2:
        raise SystemExit("fewer than two repaints recorded")
    y0 = options.y0 if options.y0 is not None else height // 10
    y1 = options.y1 if options.y1 is not None else height * 9 // 10
    # Two captures that came for one repaint show the same frame.
    frames = [
        frame
        for index, frame in enumerate(frames)
        if not (
            index and frame[0] - frames[index - 1][0] < 1.0 and frame[1] == frames[index - 1][1]
        )
    ]
    rows = []
    guess = 0
    for (before_at, before), (at, after) in zip(frames, frames[1:], strict=False):
        shift, score = shift_between(before, after, y0, y1, options.most, guess)
        if shift:
            guess = shift
        rows.append((at, at - before_at, shift, score))
    moving = [index for index, row in enumerate(rows) if row[2]]
    if not moving:
        print(f"{len(frames)} repaints, nothing moved")
        return
    first, last = moving[0], moving[-1]
    span = rows[first : last + 1]
    if options.csv:
        with open(options.csv, "w") as out:
            out.write("ms,interval_ms,shift_px,mismatch\n")
            for at, interval, shift, score in rows:
                out.write(f"{at:.3f},{interval:.3f},{shift},{score:.1f}\n")
    if not options.quiet:
        for at, interval, shift, _ in span:
            print(f"{at:9.2f} ms  +{interval:5.2f} ms  {shift:+4d} px")
    intervals = [row[1] for row in rows]
    moves = [row for row in span if row[2]]
    still = [row for row in span if not row[2]]
    # Time between moves: a repaint missed shows as a gap of two or more.
    move_times = [row[0] for row in moves]
    gaps = [b - a for a, b in zip(move_times, move_times[1:], strict=False)]
    steps = [abs(row[2]) for row in moves]
    period = statistics.median(intervals)
    print(
        f"repaints: {len(frames)}, interval median {period:.2f} ms "
        f"(p10 {quantile(intervals, 0.1):.2f} ms, p90 {quantile(intervals, 0.9):.2f} ms)"
    )
    # A capture that missed a repaint leaves a gap of two; that is the
    # recording's, not the window's.
    missed = sum(round(row[1] / period) - 1 for row in span if row[1] > period * 1.5)
    print(
        f"moving span: {span[-1][0] - span[0][0] + span[0][1]:.1f} ms "
        f"over {len(span)} repaints captured, "
        f"{len(moves)} moved, {len(still)} showed no move; {missed} "
        f"repaints not captured"
    )
    if gaps:
        late = sum(1 for gap in gaps if gap > period * 1.5)
        print(
            f"time between moves: median {statistics.median(gaps):.2f} ms, "
            f"p90 {quantile(gaps, 0.9):.2f} ms, max {max(gaps):.2f} ms; "
            f"{late} of {len(gaps)} over 1.5 repaints"
        )
    print(
        f"steps: median {statistics.median(steps)} px, max {max(steps)} px; "
        f"sequence {' '.join(str(row[2]) for row in span)}"
    )
    # Unevenness: how much each step differs from its neighbours' mean.
    jumps = [abs(b - (a + c) / 2) for a, b, c in zip(steps, steps[1:], steps[2:], strict=False)]
    if jumps:
        print(
            f"step unevenness (|step - mean of neighbours|): mean {statistics.mean(jumps):.2f} px, "
            f"max {max(jumps):.1f} px"
        )


def quantile(values, q):
    ordered = sorted(values)
    return ordered[min(len(ordered) - 1, int(q * len(ordered)))]


if __name__ == "__main__":
    sys.exit(main())
