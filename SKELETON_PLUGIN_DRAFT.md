we would like to implement Extism, https://extism.org

In the settings, there should be a plugins section, with marketplace, installed plugins and also allow git-compatible repos to install .wasm plugins manually
The system could work like how another client does it: Git repos with pinned rev commit lock.
You should be able to paste e.g. a GitHub repo in and then click install plugin.
Some sort of hot-reload feature with Extism. We don't want to restart every time we enable and disable plugins, so we need something for Slint.
The plugin system should not be enabled/load if no plugins are enabled (this way we dont take up extra memory). We also need to be memory conservative, efficient and blazingly fast. That means parallel plugin loading on startup (with dependency graphs, so we don't fuck up dependency loading order)
The plugin marketplace should have images or icons of some sort for plugins.
For the API, take a look at Vencord's: https://github.com/Vendicated/Vencord/tree/main/src/api
We want to adopt similar concepts such as Message Events, Context Menu API, etc. etc. Commands API! all of that good stuff and maybe even some more APIs?
A little side note for Extism: Extism plugins are single threaded. What Extism recommended if we need concurrency: "one pattern might be to create "pools" of plugin instances that can be called from threads running in your Host.", another way could be having the api very high-level so that we manage threading too so that we talk to the ui thread when its ui functions and when its processing stuff we do that on another thread?
The maintainer of this project suggested just having .wasm files in git repositories since they are small anyway... and we want to maintain git compatibility, not just GitHub Releases. It is your job to plan this out.
With permissions, extism plugin manifest has an allowed hosts option for connecting to https domains, we could make a popup/modal requesting the user for smth like "plugin wants to access example.com" with a allow, deny, dont ask again?
Also figure out a good default limit for how many memory pages a WASM plugin can use.
