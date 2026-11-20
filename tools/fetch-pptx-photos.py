#!/usr/bin/env python3
"""Import the documented public-domain NPS photographs into a build directory.

python tools/fetch-pptx-photos.py --output <photo-directory>

No machine-specific source path is assumed. Each NPS rights page is verified
before downloading; a failed rights check never falls back to another image.
"""
import argparse
import html
import io
import json
from pathlib import Path
import re
import urllib.request
from PIL import Image


def get(url):
    request=urllib.request.Request(url,headers={'User-Agent':'ReadMD-native-template-import/1'})
    with urllib.request.urlopen(request,timeout=180) as response:return response.read()


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output',type=Path,required=True);args=parser.parse_args()
    sources=json.loads((Path(__file__).parent/'pptx-template-photo-sources.json').read_text())
    args.output.mkdir(parents=True,exist_ok=True)
    for source in sources['sources']:
        target=args.output/source['file']
        if target.exists():
            print(source['file'],'already present',flush=True);continue
        page=get(source['source']).decode()
        region=page[page.index('Constraints Information:'):][:700]
        rights=html.unescape(re.sub('<[^>]+>',' ',region))
        if not re.search(r'public\s+domain',rights,re.I):
            raise ValueError('NPS record is not explicitly public domain: '+source['assetId'])
        im=Image.open(io.BytesIO(get(source['download']))).convert('RGB');im.info.clear()
        im.thumbnail((2400,2400),Image.Resampling.LANCZOS)
        im.save(target,quality=91,optimize=True)
        print(source['file'],im.size,'imported',flush=True)


if __name__=='__main__':main()
