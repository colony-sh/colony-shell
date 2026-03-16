//! Bundled Rack worker script for colony-shell.
//!
//! The Ruby script is embedded as a string constant and written to
//! `~/.colony/lib/rack_worker.rb` on first use. The script loads a
//! Rack app from `config.ru`, reads JSON request lines from stdin,
//! calls `app.call(env)`, and writes JSON response lines to stdout.

use std::path::PathBuf;

/// The Ruby source for the Rack worker script.
///
/// Protocol:
/// - Input (stdin): one JSON object per line with keys:
///   `method`, `path`, `query`, `host`, `port`, `headers`, `body`
/// - Output (stdout): one JSON object per line with keys:
///   `status`, `headers`, `body`
/// - Errors go to stderr (forwarded to colony's log).
pub const RACK_WORKER_SCRIPT: &str = r#"require 'rack'
require 'json'
require 'stringio'

app, _ = Rack::Builder.parse_file(ARGV[0] || 'config.ru')
STDOUT.sync = true
STDERR.sync = true

STDIN.each_line do |line|
  begin
    req = JSON.parse(line)

    rack_env = {
      'REQUEST_METHOD'    => req['method'],
      'PATH_INFO'         => req['path'],
      'QUERY_STRING'      => req['query'] || '',
      'SERVER_NAME'       => req['host'] || 'localhost',
      'SERVER_PORT'       => req['port'] || '8080',
      'SCRIPT_NAME'       => '',
      'rack.version'      => [1, 6],
      'rack.input'        => StringIO.new(req['body'] || ''),
      'rack.errors'       => STDERR,
      'rack.url_scheme'   => 'http',
      'rack.multithread'  => false,
      'rack.multiprocess' => true,
      'rack.run_once'     => false,
      'SERVER_SOFTWARE'   => 'colony-shell',
    }

    (req['headers'] || {}).each do |k, v|
      if k.downcase == 'content-type'
        rack_env['CONTENT_TYPE'] = v
      elsif k.downcase == 'content-length'
        rack_env['CONTENT_LENGTH'] = v
      else
        rack_env["HTTP_#{k.upcase.tr('-', '_')}"] = v
      end
    end

    status, headers, body = app.call(rack_env)
    chunks = []
    body.each { |chunk| chunks << chunk }
    body.close if body.respond_to?(:close)

    STDOUT.puts({ status: status, headers: headers, body: chunks.join }.to_json)
  rescue => e
    STDERR.puts("colony rack worker error: #{e.class}: #{e.message}")
    STDOUT.puts({ status: 500, headers: { 'Content-Type' => 'text/plain' }, body: "Internal Server Error: #{e.message}" }.to_json)
  end
end
"#;

/// Path where the worker script is installed.
pub fn worker_script_path() -> PathBuf {
    dirs::home_dir()
        .expect("could not determine home directory")
        .join(".colony")
        .join("lib")
        .join("rack_worker.rb")
}

/// Ensure the worker script exists at `~/.colony/lib/rack_worker.rb`.
///
/// Creates the directory and writes the script if it does not exist,
/// or if the existing script content differs (upgrade path).
pub fn ensure_worker_script() -> std::io::Result<PathBuf> {
    let path = worker_script_path();

    // Check if script already exists with correct content
    if path.exists() {
        if let Ok(existing) = std::fs::read_to_string(&path) {
            if existing == RACK_WORKER_SCRIPT {
                return Ok(path);
            }
        }
    }

    // Create parent directories
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    std::fs::write(&path, RACK_WORKER_SCRIPT)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_script_path_ends_correctly() {
        let path = worker_script_path();
        assert!(path.ends_with(".colony/lib/rack_worker.rb"));
    }

    #[test]
    fn worker_script_contains_rack_builder() {
        assert!(RACK_WORKER_SCRIPT.contains("Rack::Builder.parse_file"));
    }

    #[test]
    fn worker_script_sets_stdout_sync() {
        assert!(RACK_WORKER_SCRIPT.contains("STDOUT.sync = true"));
    }

    #[test]
    fn worker_script_handles_errors() {
        assert!(RACK_WORKER_SCRIPT.contains("rescue => e"));
        assert!(RACK_WORKER_SCRIPT.contains("status: 500"));
    }
}
