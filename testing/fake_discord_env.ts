// An environment for testing/fake_discord.ts: the content `--env PATH` puts
// in place of its built-in fixture, for demos, promo recordings and
// screenshots. PATH is a JSON file, or a TypeScript module whose default
// export is an `Env`; paths to pictures and videos in it are relative to it.
// testing/envs/example.json uses every part. Typed, a module reads:
//
//   import type { Env } from "../fake_discord_env.ts";
//   export default { me: { name: "Robin" }, people: [...] } satisfies Env;
//
// The env names people by `key`, and "me" is the account signed in, which
// keeps the fake's id for Alice. Ids come from the order things appear in,
// so they stay the same from run to run of one file.

import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";

export type Env = {
  /** The account signed in. */
  me?: Me;
  /** Everyone else, in servers or not. */
  people?: Person[];
  dms?: Dm[];
  /** Group DMs. */
  groups?: Group[];
  /** Servers, as the server list shows them, top first. */
  servers?: Server[];
  /** The GIF picker's Favorites, first first: picture files, .gif or .webp,
   *  each kept as a Tenor page whose picture it is. */
  favoriteGifs?: string[];
};

export type Status = "online" | "idle" | "dnd" | "offline";
/** `emoji` is a Unicode emoji, or a custom one in Discord's markup, `<:talk:1400000000000000300>`. */
export type CustomStatus = { text?: string; emoji?: string };

export type Me = {
  username?: string;
  /** The display name; the username without one. */
  name?: string;
  /** A picture file; without one a generated picture, with null Discord's default. */
  avatar?: string | null;
  email?: string;
  /** What they equip from the collectibles shop; see `Person`. */
  decoration?: string;
  nameplate?: Nameplate;
  style?: DisplayNameStyle;
  /** The SKU of the profile effect they equip; see `Person`. */
  effect?: string;
  nitro?: boolean;
  status?: Status;
  customStatus?: CustomStatus;
};

/** A nameplate: the SKU it was sold as, which names its picture, and a palette such as "violet", "teal" or "bubble_gum". */
export type Nameplate = { sku: string; palette: string };

/** A display name style: Discord's font id (11 is the usual font; 3 Sakura, 4 Jellybean, 6 Modern, 7 Medieval, 8 8Bit,
 *  10 Vampyre, 12 Tempo, 13 Monkey Bars, 14 Mainframe, 15 Headbang, 16 Journal), effect id (1 solid, 2 gradient, 3 neon,
 *  4 toon, 5 pop, 6 glow, 7 prism, 8 gummy) and RGB colours as numbers. */
export type DisplayNameStyle = { font: number; effect: number; colors: number[] };

export type Person = {
  /** How the env names them: in `from`, `with`, `who`, reactions' `by`, and `<@key>`. */
  key: string;
  /** Their username; the key without one. */
  username?: string;
  name?: string;
  avatar?: string | null;
  /** The SKU of the avatar decoration they wear: a ring around their avatar. */
  decoration?: string;
  nameplate?: Nameplate;
  style?: DisplayNameStyle;
  /** The SKU of a profile effect they equip, such as "1500000000000000010", the one the fake serves: layers over their profile card. */
  effect?: string;
  bot?: boolean;
  /** "online" without one. */
  status?: Status;
  /** A game they're playing, shown as "Playing …". */
  playing?: string;
  customStatus?: CustomStatus;
  /** Online on their phone only. */
  mobile?: boolean;
  relationship?: "friend" | "incoming" | "outgoing" | "blocked";
  bio?: string;
  pronouns?: string;
  /** A picture file for their profile's banner. */
  banner?: string;
  /** Their profile's colour without a banner, as "#5865f2". */
  accentColor?: string;
};

export type Message = {
  /** A person's key, or "me". */
  from: string;
  /** Discord markdown. `<@key>` mentions a person, `<#name>` a channel of
   *  the same server, `<@&name>` one of its roles. */
  text?: string;
  /** When it was sent: an ISO time such as "2026-10-14T18:30:00Z", or a
   *  time before the fake started, such as "45m ago", "3h ago" or
   *  "2d 4h 10m ago". Without one, a minute after the message before; a
   *  channel's first without one is as many minutes ago as the channel has
   *  messages, so a channel of untimed messages ends a minute ago. */
  at?: string;
  /** A name for `reply` to refer to. */
  id?: string;
  /** The `id` of the message in the same channel it replies to. */
  reply?: string;
  reactions?: Reaction[];
  /** Picture and video files, or other files, attached. */
  files?: (string | Attachment)[];
  pinned?: boolean;
};

export type Reaction = {
  /** A Unicode emoji. */
  emoji: string;
  /** Who reacted: keys, "me" for the account's own. */
  by: string[];
};

export type Attachment = {
  path: string;
  /** The name it's shown with; the file's own without one. */
  name?: string;
  spoiler?: boolean;
};

/** Read state: how many of the newest messages are unread, and how many
 *  mentions count. In DMs and groups the unread ones count as mentions,
 *  as on Discord, unless `mentions` says otherwise. */
export type Unread = { unread?: number; mentions?: number };

export type Dm = Unread & { with: string; messages?: Message[] };
export type Group = Unread & { name?: string; with: string[]; messages?: Message[] };

export type Server = {
  name: string;
  /** A picture file. */
  icon?: string;
  /** A person's key, or "me", the default. */
  owner?: string;
  /** Highest first. @everyone is there anyway. */
  roles?: Role[];
  /** Keys, or members with roles and nicknames. The account, the owner,
   *  and everyone who writes or sits in voice there are members anyway. */
  members?: (string | Member)[];
  /** Channels outside any category, above the categories. */
  channels?: Channel[];
  categories?: { name: string; channels: Channel[] }[];
};

export type Role = {
  name: string;
  /** As "#e91e63". */
  color?: string;
  /** Listed on its own in the member list. */
  hoist?: boolean;
  mentionable?: boolean;
  /** A permission bitfield, as a decimal string; none without one. */
  permissions?: string;
};

export type Member = { who: string; roles?: string[]; nick?: string };

export type Channel = TextChannel | VoiceChannel;
export type TextChannel = Unread & { name: string; type?: "text"; topic?: string; messages?: Message[] };
export type VoiceChannel = { name: string; type: "voice"; people?: InVoice[] };

export type InVoice = {
  who: string;
  mute?: boolean;
  deaf?: boolean;
  /** Their camera is on. The fake sends no camera video for env people. */
  video?: boolean;
  /** Streaming with Go Live: a video file a viewer is sent, or true for
   *  testing/fixtures' recording. */
  stream?: string | true;
  /** Muted or deafened by a moderator. */
  serverMute?: boolean;
  serverDeaf?: boolean;
};

/** A picture or video an env names, read and measured. */
export type Media = {
  data: Uint8Array;
  contentType: string;
  width?: number;
  height?: number;
  /** A video's first frame, as a PNG, for the media proxy's poster. */
  poster?: Uint8Array;
};

/** The frames a viewer of a stream is sent: the video at STREAM_FPS, as
 *  VP8 in IVF and as H264 Annex B with access unit delimiters, the forms of
 *  testing/fixtures' stream-vp8.ivf and stream-h264.h264. */
export type Recording = { vp8: Buffer; h264: Buffer; fps: number };

/** An env stream's frame rate: smooth enough to show a game. testing/
 *  fixtures' own recording stays at 10, as tests expect. */
export const STREAM_FPS = 30;

/** Reads the env at `path`, relative to the working directory. */
export async function readEnv(path: string): Promise<{ env: Env; dir: string }> {
  const file = resolve(path);
  const module = await import(file);
  return { env: (module.default ?? module) as Env, dir: dirname(file) };
}

const TYPES: Record<string, string> = {
  png: "image/png",
  jpg: "image/jpeg",
  jpeg: "image/jpeg",
  gif: "image/gif",
  webp: "image/webp",
  mp4: "video/mp4",
  webm: "video/webm",
  mov: "video/quicktime",
  txt: "text/plain; charset=utf-8",
};

/** Where converted media is kept, by a hash of what it came from. */
const CACHE = join(tmpdir(), "yapfast-fake-discord-env");

function run(command: string[]): Buffer {
  const done = Bun.spawnSync(command, { stdout: "pipe", stderr: "pipe" });
  if (done.exitCode !== 0) throw new Error(`${command[0]} failed on ${command.at(-1)}: ${done.stderr.toString().slice(-400)}`);
  return done.stdout;
}

/** The output of `make(file)` for this input, made once and kept in CACHE. */
function cached(data: Uint8Array, name: string, make: (file: string) => void): Buffer {
  mkdirSync(CACHE, { recursive: true });
  const file = join(CACHE, `${createHash("sha256").update(data).digest("hex").slice(0, 24)}-${name}`);
  if (!existsSync(file)) make(file);
  return readFileSync(file);
}

/** Reads a picture or video, with its size from ffprobe and, for a video,
 *  its first frame. */
export function readMedia(dir: string, path: string): Media {
  const file = resolve(dir, path);
  const data = new Uint8Array(readFileSync(file));
  const contentType = TYPES[path.split(".").pop()!.toLowerCase()] ?? "application/octet-stream";
  if (!contentType.startsWith("image/") && !contentType.startsWith("video/")) return { data, contentType };
  const probed = run(["ffprobe", "-v", "error", "-select_streams", "v:0", "-show_entries", "stream=width,height", "-of", "csv=p=0", file]);
  const [width, height] = probed.toString().trim().split(",").map(Number);
  const media: Media = { data, contentType, width, height };
  if (contentType.startsWith("video/")) {
    media.poster = new Uint8Array(cached(data, "poster.png", (out) => {
      run(["ffmpeg", "-v", "error", "-y", "-i", file, "-frames:v", "1", out]);
    }));
  }
  return media;
}

/** The recording a stream of this video sends, converted with ffmpeg the
 *  first time. Frames go out at STREAM_FPS, with a keyframe every second. */
export function readRecording(dir: string, path: string): Recording {
  const file = resolve(dir, path);
  const data = readFileSync(file);
  const rate = String(STREAM_FPS);
  const video = ["-v", "error", "-y", "-i", file, "-an", "-r", rate, "-g", rate, "-pix_fmt", "yuv420p"];
  const vp8 = cached(data, `stream-${rate}.ivf`, (out) => {
    run(["ffmpeg", ...video, "-c:v", "libvpx", "-b:v", "1500k", "-auto-alt-ref", "0", "-f", "ivf", out]);
  });
  const h264 = cached(data, `stream-${rate}.h264`, (out) => {
    run(["ffmpeg", ...video, "-c:v", "libx264", "-profile:v", "baseline", "-bsf:v", "h264_metadata=aud=insert", "-f", "h264", out]);
  });
  return { vp8, h264, fps: STREAM_FPS };
}

/** When a message was sent, in Unix milliseconds: an ISO time, or "… ago"
 *  from `now`. */
export function parseTime(at: string, now: number): number {
  const ago = at.trim().match(/^(?:(\d+)d)?\s*(?:(\d+)h)?\s*(?:(\d+)m)?\s*(?:(\d+)s)?\s*ago$/);
  if (ago && ago.slice(1).some((part) => part !== undefined)) {
    const [days, hours, minutes, seconds] = ago.slice(1).map((part) => Number(part ?? 0));
    return now - (((days! * 24 + hours!) * 60 + minutes!) * 60 + seconds!) * 1000;
  }
  const ms = Date.parse(at);
  if (Number.isNaN(ms)) throw new Error(`env: "${at}" is neither an ISO time nor such as "5m ago"`);
  return ms;
}

/** A colour such as "#e91e63" as Discord's integer. */
export function parseColor(color: string | undefined): number {
  if (!color) return 0;
  const hex = color.replace(/^#/, "");
  if (!/^[0-9a-f]{6}$/i.test(hex)) throw new Error(`env: colour "${color}" isn't #rrggbb`);
  return parseInt(hex, 16);
}

