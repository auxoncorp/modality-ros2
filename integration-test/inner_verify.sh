set -ex

modality workspace sync-indices
modality query *@*

for s in /specs/*; do
    conform spec eval --file ${s} --dry-run
done
