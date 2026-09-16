---
status: accepted
---

# Use organization-managed signing identities

Production release, policy and baseline signatures use distinct organization-managed KMS/HSM identities, while the pilot may use isolated protected-CI keys with the same identity separation. Authors and Factory Workers never receive signing authority, and runners trust configured organizational roots rather than developer identities or ordinary repository secrets.
