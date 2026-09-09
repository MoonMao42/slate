//! Native comma-separated config chains, without tool execution at startup.
//! Rebuild only Slate's known values. An explicit custom LG_CONFIG_FILE wins.
use super::ShellIntegrationOptions;

#[derive(Debug, Clone)]
pub(super) struct Config {
    managed: String,
    user: String,
    merged: String,
    legacy: String,
    representable: bool,
}

impl Config {
    pub fn new(options: &ShellIntegrationOptions<'_>, quote: fn(&str) -> String) -> Self {
        let managed = format!("{}/lazygit/config.yml", options.managed_root);
        let user = options.lazygit_default_config;
        Self {
            merged: quote(&format!("{managed},{user}")),
            legacy: quote(&format!(
                "{managed}:{}/lazygit/config.yml",
                options.user_config_root
            )),
            representable: !managed.contains(',') && !user.contains(','),
            managed: quote(&managed),
            user: quote(user),
        }
    }

    pub fn posix(&self, content: &mut String) {
        if !self.representable {
            content.push_str("# Lazygit config path contains a comma; leave native config selection unchanged.\n");
            return;
        }
        let Self {
            managed,
            user,
            merged,
            legacy,
            ..
        } = self;
        content.push_str(&format!(
            r#"
# Lazygit: personal config overrides palette; custom LG_CONFIG_FILE is untouched.
case "${{LG_CONFIG_FILE:-}}" in
  ''|{managed}|{merged}|{legacy})
    if [ -f {managed} ] && [ -r {managed} ]; then
      if [ -f {user} ] && [ -r {user} ]; then
        export LG_CONFIG_FILE={merged}
      elif [ ! -e {user} ] && [ ! -L {user} ]; then
        export LG_CONFIG_FILE={managed}
      else
        # An unsafe/unreadable personal config must not be silently bypassed.
        unset LG_CONFIG_FILE
      fi
    else
      unset LG_CONFIG_FILE
    fi
    ;;
esac
"#
        ));
    }

    pub fn fish(&self, content: &mut String) {
        if !self.representable {
            content.push_str("# Lazygit config path contains a comma; leave native config selection unchanged.\n");
            return;
        }
        let Self {
            managed,
            user,
            merged,
            legacy,
            ..
        } = self;
        content.push_str(&format!(
            r#"
# Lazygit: personal config overrides palette; custom LG_CONFIG_FILE is untouched.
if not set -q LG_CONFIG_FILE[2]
  if not set -q LG_CONFIG_FILE[1]; or contains -- "$LG_CONFIG_FILE" '' {managed} {merged} {legacy}
    if test -f {managed}; and test -r {managed}
      if test -f {user}; and test -r {user}
        set -gx LG_CONFIG_FILE {merged}
      else if not test -e {user}; and not test -L {user}
        set -gx LG_CONFIG_FILE {managed}
      else
        set -e LG_CONFIG_FILE; or true
      end
    else
      set -e LG_CONFIG_FILE; or true
    end
  end
end
"#
        ));
    }
}

#[cfg(test)]
mod tests;
