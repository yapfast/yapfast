//! Samples of the authorization dialog for tests and shots, after the fake
//! Discord's data: task N12's.

use yapfast_lang::tr;

use crate::Vm;
use crate::fixtures::{avatar, shell};
use crate::places::oauth::{App, OAuth, Servers};
use crate::shell::Layer;

/// An application asking for its scopes, to a server the link names.
pub fn asking() -> OAuth {
    OAuth {
        title: tr!("Authorize {}", "Status Bot"),
        app: Some(App {
            name: "Status Bot".into(),
            avatar: avatar("S", 0x5865f2),
            detail: "Posts deploys.".into(),
        }),
        wants: "- Read your username\n- Add a bot to a server".into(),
        server: "Server".into(),
        confirm: tr!("Authorize").into(),
        ..OAuth::default()
    }
}

/// A server install the person picks the server of.
pub fn picking() -> OAuth {
    OAuth {
        server: String::new(),
        servers: Some(Servers {
            names: vec!["Server".into(), "Second".into(), "Third".into()],
            chosen: 0,
        }),
        ..asking()
    }
}

/// It asks for Administrator: the warning shows in red.
pub fn dangerous() -> OAuth {
    OAuth {
        warning: tr!(
            "Its bot asks for Administrator: every permission in the server, in every channel, whatever its roles allow."
        )
        .into(),
        ..picking()
    }
}

/// Authorized, with an address to copy.
pub fn done() -> OAuth {
    OAuth {
        confirm: String::new(),
        address: "myapp://callback?code=abc".into(),
        status: tr!(
            "Authorized. Discord sends you on to this address, which isn't a web page, so Yapfast doesn't open it. Copy it if the app asks for it."
        )
        .into(),
        ..asking()
    }
}

/// Authorizing: the buttons wait.
pub fn authorizing() -> OAuth {
    OAuth {
        busy: true,
        status: tr!("Authorizing...").into(),
        ..picking()
    }
}

/// Looking it up: nothing found yet.
pub fn looking() -> OAuth {
    OAuth {
        title: tr!("Authorize").into(),
        busy: true,
        status: tr!("Looking up the application...").into(),
        ..OAuth::default()
    }
}

/// It failed.
pub fn failed() -> OAuth {
    OAuth {
        title: tr!("Authorize").into(),
        error: "This application does not exist.".into(),
        ..OAuth::default()
    }
}

/// The window on the first server with the dialog open on `oauth`.
pub fn over(oauth: OAuth) -> Vm {
    let mut vm = shell::open(shell::server_home(), Layer::OAuth);
    vm.places.oauth = oauth;
    vm
}
