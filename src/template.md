# Acknowledgements

I hereby express my sincere gratitude and appreciation for the code contributions made by other individuals to my direct dependencies. Without their tireless efforts, my work would not be possible, and I am deeply grateful for their contributions to the advancement of our collective knowledge.

## Thank you!

{{#each thank}}
  {{#if name_and_count}}
- **[@{{name}}]({{profile_url}})** for their {{count}} contributions
  {{/if}}
  {{#if dep_and_names}}
- Contributors of `{{crate_name}}`: {{#each contributors}} **[@{{name}}]({{profile_url}})**{{#unless @last}}, {{/unless}}{{/each}}
  {{/if}}
  {{#if name_and_deps}}
- **[@{{name}}]({{profile_url}})** for their conributions to: {{#each crates}}`{{this}}`{{#unless @last}}, {{/unless}}{{/each}}
  {{/if}}
{{/each}}

{{#if others}}
And {{others}} others for their contributions which didn't make it to this list.
{{/if}}
