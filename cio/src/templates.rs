use handlebars::{Context, Handlebars, Helper, HelperResult, Output, RenderContext};
use hubcaps::repositories::Repository;
use tracing::instrument;

use crate::shorturls::ShortUrl;
use crate::utils::create_or_update_file_in_github_repo;

/// Helper function so the terraform names do not start with a number.
/// Otherwise terraform will fail.
fn terraform_name_helper(h: &Helper, _: &Handlebars, _: &Context, _rc: &mut RenderContext, out: &mut dyn Output) -> HelperResult {
    let p = h.param(0).unwrap().value().to_string();
    let param = p.trim_matches('"');

    // Check if the first character is a number.
    let first_char = param.chars().next().unwrap();
    if first_char.is_digit(10) {
        out.write(&("_".to_owned() + param))?;
    } else {
        out.write(&param)?;
    }
    Ok(())
}

/// Generate nginx and terraform files for shorturls.
/// This is used for short URL link generation like:
///   - {link}.corp.oxide.computer
///   - {repo}.git.oxide.computer
///   - {num}.rfd.oxide.computer
/// This function saves the generated files in the GitHub repository, in the
/// given path.
#[instrument(skip(repo))]
#[inline]
pub async fn generate_nginx_and_terraform_files_for_shorturls(repo: &Repository, shorturls: Vec<ShortUrl>) {
    if shorturls.is_empty() {
        println!("no shorturls in array");
        return;
    }

    let r = repo.get().await.unwrap();

    // Initialize handlebars.
    let mut handlebars = Handlebars::new();
    handlebars.register_helper("terraformize", Box::new(terraform_name_helper));

    // Get the subdomain from the first link.
    let subdomain = shorturls[0].subdomain.to_string();

    // Generate the subdomains nginx file.
    let nginx_file = format!("/nginx/conf.d/generated.{}.oxide.computer.conf", subdomain);
    // Add a warning to the top of the file that it should _never_
    // be edited by hand and generate it.
    let mut nginx_rendered = TEMPLATE_WARNING.to_owned() + &handlebars.render_template(&TEMPLATE_NGINX, &shorturls).unwrap();
    // Add the vim formating string.
    nginx_rendered += "# vi: ft=nginx";

    create_or_update_file_in_github_repo(repo, &r.default_branch, &nginx_file, nginx_rendered.as_bytes().to_vec()).await;

    // Generate the paths nginx file.
    let nginx_paths_file = format!("/nginx/conf.d/generated.{}.paths.oxide.computer.conf", subdomain);
    // Add a warning to the top of the file that it should _never_
    // be edited by hand and generate it.
    let mut nginx_paths_rendered = TEMPLATE_WARNING.to_owned() + &handlebars.render_template(&TEMPLATE_NGINX_PATHS, &shorturls).unwrap();
    // Add the vim formating string.
    nginx_paths_rendered += "# vi: ft=nginx";

    create_or_update_file_in_github_repo(repo, &r.default_branch, &nginx_paths_file, nginx_paths_rendered.as_bytes().to_vec()).await;

    generate_terraform_files_for_shorturls(repo, shorturls).await;
}

/// Generate terraform files for shorturls.
/// This function saves the generated files in the GitHub repository, in the
/// given path.
#[instrument(skip(repo))]
#[inline]
pub async fn generate_terraform_files_for_shorturls(repo: &Repository, shorturls: Vec<ShortUrl>) {
    if shorturls.is_empty() {
        println!("no shorturls in array");
        return;
    }

    let r = repo.get().await.unwrap();

    // Initialize handlebars.
    let mut handlebars = Handlebars::new();
    handlebars.register_helper("terraformize", Box::new(terraform_name_helper));

    // Get the subdomain from the first link.
    let subdomain = shorturls[0].subdomain.to_string();

    // Generate the terraform file.
    let terraform_file = format!("/terraform/cloudflare/generated.{}.oxide.computer.tf", subdomain);
    // Add a warning to the top of the file that it should _never_
    // be edited by hand and generate it.
    let terraform_rendered = TEMPLATE_WARNING.to_owned() + &handlebars.render_template(&TEMPLATE_CLOUDFLARE_TERRAFORM, &shorturls).unwrap();

    create_or_update_file_in_github_repo(repo, &r.default_branch, &terraform_file, terraform_rendered.as_bytes().to_vec()).await;
}

/// The warning for files that we automatically generate so folks don't edit them
/// all willy nilly.
pub static TEMPLATE_WARNING: &str = "# THIS FILE HAS BEEN GENERATED BY THE CIO REPO
# AND SHOULD NEVER BE EDITED BY HAND!!
# Instead change the link in configs/links.toml
";

/// Template for creating nginx conf files for the subdomain urls.
pub static TEMPLATE_NGINX: &str = r#"{{#each this}}
# Redirect {{this.link}} to {{this.name}}.{{this.subdomain}}.oxide.computer
# Description: {{this.description}}
server {
	listen      [::]:443 ssl http2;
	listen      443 ssl http2;
	server_name {{this.name}}.{{this.subdomain}}.oxide.computer;

	include ssl-params.conf;

	ssl_certificate			/etc/nginx/ssl/wildcard.{{this.subdomain}}.oxide.computer/fullchain.pem;
	ssl_certificate_key		/etc/nginx/ssl/wildcard.{{this.subdomain}}.oxide.computer/privkey.pem;
	ssl_trusted_certificate	    	/etc/nginx/ssl/wildcard.{{this.subdomain}}.oxide.computer/fullchain.pem;

	# Add redirect.
	location / {
		return 301 "{{this.link}}";
	}

	{{#if this.discussion}}# Redirect /discussion to {{this.discussion}}
	# Description: Discussion link for {{this.description}}
	location /discussion {
		return 301 {{this.discussion}};
	}
{{/if}}
}
{{/each}}
"#;

/// Template for creating nginx conf files for the paths urls.
pub static TEMPLATE_NGINX_PATHS: &str = r#"server {
	listen      [::]:443 ssl http2;
	listen      443 ssl http2;
	server_name {{this.0.subdomain}}.oxide.computer;

	include ssl-params.conf;

	# Note this certificate is NOT the wildcard, since these are paths.
	ssl_certificate			/etc/nginx/ssl/{{this.0.subdomain}}.oxide.computer/fullchain.pem;
	ssl_certificate_key		/etc/nginx/ssl/{{this.0.subdomain}}.oxide.computer/privkey.pem;
	ssl_trusted_certificate	        /etc/nginx/ssl/{{this.0.subdomain}}.oxide.computer/fullchain.pem;

	location = / {
		return 301 https://github.com/oxidecomputer/meta/tree/master/links;
	}

	{{#each this}}
	# Redirect {{this.subdomain}}.oxide.computer/{{this.name}} to {{this.link}}
	# Description: {{this.description}}
	location = /{{this.name}} {
		return 301 "{{this.link}}";
	}
{{#if this.discussion}}	# Redirect /{{this.name}}/discussion to {{this.discussion}}
	# Description: Discussion link for {{this.name}}
	location = /{{this.name}}/discussion {
		return 301 {{this.discussion}};
	}
{{/if}}
{{/each}}
}
"#;

/// Template for creating DNS records in our Cloudflare terraform configs.
pub static TEMPLATE_CLOUDFLARE_TERRAFORM: &str = r#"{{#each this}}
resource "cloudflare_record" "{{terraformize this.name}}_{{this.subdomain}}_oxide_computer" {
  zone_id  = var.zone_id-oxide_computer
  name     = "{{this.name}}.{{this.subdomain}}.oxide.computer"
  value    = {{{this.ip}}}
  type     = "A"
  ttl      = 1
  priority = 0
  proxied  = false
}
{{/each}}
"#;

/// Template for the groups table.
pub static TEMPLATE_TABLE_GROUPS: &str = "<!-- THIS WHOLE FILE IS GENERATED FROM THE CIO REPO in cio/src/templates.rs DO NOT EDIT IT DIRECTLY -->
# Groups
Mailing list groups for the company.
This is also in a [Airtable workspace](https://airtable.com/tbluawHpHEo0wT9Ky/viwy0bTXGCGlkXlIZ?blocks=hide).
<!-- START doctoc -->
<!-- END doctoc -->
| Name | Email | Description | External Members Allowed? | Who can post | Who can view | Who can join | Who can discover |
| ---- | ----- | ----------- | ------------------------- |--------------|--------------|--------------|------------------|{{#each this.groups}}
| [{{ this.name }}](https://groups.google.com/a/oxidecomputer.com/forum/#!forum/{{ this.name }}) | [`{{ this.name }}@oxide.computer`](mailto:{{ this.name }}@oxide.computer) | {{ this.description }} | `{{ this.allow_external_members }}` | `{{ this.who_can_post_message }}` | `{{ this.who_can_view_group }}` | `{{ this.who_can_join }}` | `{{ this.who_can_discover_group }}` |{{/each}}
To edit anything in this table, email [jess@oxide.computer](mailto:jess@oxide.computer).
This whole file is automatically generated by `configs` from
[oxidecomputer/configs](https://github.com/oxidecomputer/configs), DO NOT edit
it directly.";

/// Template for the links table.
pub static TEMPLATE_TABLE_LINKS: &str = "<!-- THIS WHOLE FILE IS GENERATED FROM THE CIO REPO in cio/src/templates.rs DO NOT EDIT IT DIRECTLY -->
# Links
Helpful links and resources for members of the company.
<!-- START doctoc -->
<!-- END doctoc -->
## GitHub Repos
We have a nice way to easily link to any repository we have in the
`oxidecomputer` organization on GitHub.
```
https://{repo}.git.oxide.computer
```
OR
```
https://git.oxide.computer/{repo}
```
An example of this is: [tockilator.git.oxide.computer](https://tockilator.git.oxide.computer)
## RFDs
We have a nice way to easily link to any RFD in the [rfd](https://github.com/oxidecomputer/rfd) repo. This link updates when the file is merged into master, so you can consider it a safe link to use to link to an RFD that might move from a branch to master after being merged.
```
https://{number}.rfd.oxide.computer
```
OR
```
https://rfd.oxide.computer/{number}
```
An example of this is: [4.rfd.oxide.computer](https://4.rfd.oxide.computer)
If you are trying to get to the pull request for the RFD you can use:
```
https://{number}.rfd.oxide.computer/discussion
```
OR
```
https://rfd.oxide.computer/{number}/discussion
```
You can also view rendered versions of RFDs at [https://rfd.shared.oxide.computer/](https://rfd.shared.oxide.computer).  For more on this, see [this README](https://github.com/oxidecomputer/rfd/blob/rendered/README.md).
## Internal Resources
| Link | Description | Aliases |
| ---- | ----------- |---------|{{#each this.links}}
| [{{ @key }}.corp.oxide.computer](https://{{ @key }}.corp.oxide.computer) | {{ this.description }} | {{#each this.aliases}}[{{this}}.corp.oxide.computer](https://{{this}}.corp.oxide.computer) {{/each}} |{{/each}}
To edit anything in this table, email [jess@oxide.computer](mailto:jess@oxide.computer).
This whole file is automatically generated by `configs` from
[oxidecomputer/configs](https://github.com/oxidecomputer/configs), DO NOT edit
it directly. Instead make your changes to the [links.toml](https://github.com/oxidecomputer/configs/blob/master/configs/links.toml) file.";

/// Template for the people table.
pub static TEMPLATE_TABLE_PEOPLE: &str = "<!-- THIS WHOLE FILE IS GENERATED FROM THE CIO REPO in cio/src/templates.rs DO NOT EDIT IT DIRECTLY -->
# People
Members of the company.
This is also in a [Airtable workspace](https://airtable.com/tbl9hAIRMwKm526xj/viwSAemyFvIyZFxuB?blocks=hide).
<!-- START doctoc -->
<!-- END doctoc -->
| Name | Email | Email Aliases | GitHub | Chat | Twitter |
| ---- | ----- | ------------- | ------ | ---- | ------- |{{#each this.users}}
| {{ this.first_name }} {{ this.last_name }} | [`{{ this.username }}@oxide.computer`](mailto:{{ this.username }}@oxide.computer) | `{{ this.aliases }}` | {{#if this.github}}[@{{ this.github }}](https://github.com/{{ this.github }}){{/if}} | {{#if this.chat}}`{{ this.chat }}`{{/if}} | {{#if this.twitter}}[@{{ this.twitter }}](https://twitter.com/{{ this.twitter }}){{/if}} |{{/each}}
To edit anything in this table, email [jess@oxide.computer](mailto:jess@oxide.computer).
This whole file is automatically generated by `configs` from
[oxidecomputer/configs](https://github.com/oxidecomputer/configs), DO NOT edit
it directly.";
