# Kubernetes controller to generate forward and reverse DNS records for all nodes on Linode Kubernetes

This controller uses Linode API to automatically create or update DNS records for Kubernetes nodes in your Linode cluster.

By default, Linode creates reverse DNS records under `ip.linodeusercontent.com` domain, which is not the best approach in many cases:
1. Hard to identify your own nodes vs. some other hosts, when troubleshooting any _outgoing_ connections from these nodes.
2. Outgoing SMTP is problematic in particular: it's best to use own domain to shield from potential issues with `linodeusercontent.com` reputation,
   defining SPF records is also much easier when the nodes belong to your own domain.

## Howto

1. Create a secondary domain in Linode control panel, under a domain that you already own. For example, `k8s.example.com`.
2. Using your current DNS provider for your top-level `example.com` domain, add NS records pointing to Linode nameservers for `k8s.example.com` domain:
    ```
        k8s NS ns1.linode.com.
        k8s NS ns2.linode.com.
        k8s NS ns3.linode.com.
        k8s NS ns4.linode.com.
        k8s NS ns5.linode.com.
    ```
3. Create a service account, a cluster role to read and watch nodes, and bind it to the service account:
```yaml
apiVersion: v1
kind: ServiceAccount
metadata:
  name: node-dns
  namespace: default
automountServiceAccountToken: true
---
apiVersion: rbac.authorization.k8s.io/v1
kind: ClusterRole
metadata:
   name: node-dns
rules:
   - apiGroups:
        - ""
     resources:
        - nodes
     verbs:
        - get
        - list
        - watch
---
apiVersion: rbac.authorization.k8s.io/v1
kind: ClusterRoleBinding
metadata:
   name: node-dns
roleRef:
   apiGroup: rbac.authorization.k8s.io
   kind: ClusterRole
   name: node-dns
subjects:
   - kind: ServiceAccount
     name: node-dns
     namespace: default
```
4. Create a [Linode API personal access token](https://cloud.linode.com/profile/tokens). 
The required scopes are read/write domains and IPs. Then, create a Kubernetes secret with this token:
```bash
kubectl create secret generic linode-api-token --from-literal=token=$LINODE_API_TOKEN
```

5. Create a deployment with this controller, expose the API token and domain as environment variables:
```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: node-dns
  namespace: default
  labels:
    app.kubernetes.io/name: node-dns
    app.kubernetes.io/component: controller
spec:
  replicas: 1
  selector:
    matchLabels:
      app.kubernetes.io/name: node-dns
      app.kubernetes.io/component: controller
  template:
    metadata:
      name: node-dns-controller
      labels:
        app.kubernetes.io/name: node-dns
        app.kubernetes.io/component: controller
    spec:
      containers:
        - name: node-dns-controller
          image: ghcr.io/haimgel/lke-node-dns:0.1.0
          imagePullPolicy: Always
          command: ["/app/node-dns"]
          env:
            - name: NODE_DOMAIN
              value: "k8s.example.com"
            - name: LINODE_API_TOKEN
              valueFrom:
                secretKeyRef:
                  key: token
                  name: linode-api-token
      serviceAccount: node-dns
      automountServiceAccountToken: true
```

Once deployed, watch the logs to verify that the controller is working as expected.

## Cleanup

This controller adds a finalizer to each node to delete the DNS records when the node is deleted. If you stopped using
this controller, please remove the finalizer `k8s.haim.dev/linode-dns-finalizer` manually from each node, otherwise
the nodes won't be cleaned up properly. They'll be stuck in `NotReady,SchedulingDisabled` state till the finalizer is 
removed.
