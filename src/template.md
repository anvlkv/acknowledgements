# Acknowledgements

I hereby express my sincere gratitude and appreciation for the code contributions made by other individuals to my direct dependencies. Without their tireless efforts, my work would not be possible, and I am deeply grateful for their contributions to the advancement of our collective knowledge.

## Thank you 🙏

{{#each thank}}
  {{#if NameAndCount}}
- {{#if NameAndCount.profile_url}}**[{{#if ../../mention}}@{{/if}}{{NameAndCount.name}}]({{NameAndCount.profile_url}})**{{else}}**{{NameAndCount.name}}**{{/if}} for their {{NameAndCount.count}} {{plural NameAndCount.count 'contribution' 'contributions'}}
  {{/if}}
  {{#if DepAndNames}}
- Contributors of `{{DepAndNames.crate_name}}`: {{#each DepAndNames.contributors}} {{#if this.[1]}}**[{{#if ../../mention}}@{{/if}}{{this.[0]}}]({{this.[1]}})**{{else}}**{{this.[0]}}**{{/if}}{{#unless @last}}, {{/unless}}{{/each}}
  {{/if}}
  {{#if NameAndDeps}}
- {{#if NameAndDeps.profile_url}}**[{{#if ../../mention}}@{{/if}}{{NameAndDeps.name}}]({{NameAndDeps.profile_url}})**{{else}}**{{NameAndDeps.name}}**{{/if}} for their conributions to: {{#each NameAndDeps.crates}}`{{this}}`{{#unless @last}}, {{/unless}}{{/each}}
  {{/if}}
{{/each}}

{{#if others}}
And {{others}} other cool humans for their contributions, who haven't made it to this list yet.
{{/if}}

---

This file was generated using [`acknowldge` CLI](https://crates.io/crates/acknowledgements-rs). Thank you for acknowledging the contributors!
