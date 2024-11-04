#!/bin/sh

set -e
cd `dirname "$0"`

curl 'ftp://ftp.cisjr.cz/netex/Netex_VerejnaLinkovaDoprava.zip' --output spoje1.zip
curl 'ftp://ftp.cisjr.cz/netex/NeTEx_GVD2024.zip' --output spoje2.zip
curl 'ftp://ftp.cisjr.cz/netex/NeTEx_DrahyMestske.zip' --output spoje3.zip

echo extracting

for FILE in spoje*.zip; do
	unzip -q "$FILE"
	rm "$FILE"
done

echo done
