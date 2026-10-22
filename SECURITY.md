# Security

## Reporting a vulnerability

Please report security problems privately: on the repository's **Security**
tab, choose **Report a vulnerability**
(https://github.com/braveadmin/kube-city/security/advisories/new). Do not
open a public issue. You will get an answer as soon as possible, and credit
in the fix if you want it.

## Supported versions

Only the latest release gets fixes.

## What kube-city does

- It only lists and watches pods through your kubeconfig; it never changes
  anything in a cluster.
- `--web` serves its page on 127.0.0.1 only, without authentication: any
  program on the same machine can read the city, including namespace and
  workload names.
- Recordings (`--record`) contain namespace and workload names; treat them
  like the cluster information they are.
