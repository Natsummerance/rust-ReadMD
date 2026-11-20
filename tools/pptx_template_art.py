"""Original chrome fluid and documented NPS photo import for native art seeds.

Inputs are build-time directories supplied explicitly to the compiler. Result
packages contain only converted art and public provenance, never input paths.
"""
import io
import hashlib
import json
import posixpath
from pathlib import Path
from xml.dom import Node

from defusedxml import minidom
from PIL import Image, ImageDraw, ImageFilter

REL='http://schemas.openxmlformats.org/package/2006/relationships'
OFFICE_REL='http://schemas.openxmlformats.org/officeDocument/2006/relationships'


def first(node,tag):
    return next(iter(node.getElementsByTagName(tag)),None)


def label(node):
    p=first(node,'p:cNvPr')
    return p.getAttribute('name') if p else ''


def encode(im,format='PNG'):
    im.info.clear();out=io.BytesIO()
    if format=='JPEG':im.convert('RGB').save(out,format,quality=93,optimize=True)
    else:im.save(out,format,optimize=True)
    return out.getvalue()


def cropped_photo(path,aspect,size,trim,center_y=.53):
    im=Image.open(path).convert('RGB');im.info.clear()
    w,h=im.size;l,t,r,b=trim
    im=im.crop((round(w*l),round(h*t),round(w*(1-r)),round(h*(1-b))))
    w,h=im.size
    if w/h>aspect:
        width=h*aspect;x=(w-width)/2;box=(x,0,x+width,h)
    else:
        height=w/aspect;y=max(0,min(h-height,h*center_y-height/2));box=(0,y,w,y+height)
    return im.crop(box).resize(size,Image.Resampling.LANCZOS)


def coastal_foreground(im,slide):
    """Fresh mattes traced against the exact documented photo crops.

    Only the nearest coastal headland/rock belongs to the foreground. The
    distant right-hand shore stays in the plate, so a horizon-shaped matte
    cannot cut the ribbon off with a conspicuous horizontal seam.
    Supersampling keeps edges continuous without color-key holes in sea foam.
    """
    rgb=im.convert('RGB');w,h=rgb.size;scale=3
    if slide==1:
        points=[(.477,1),(.501,.980),(.526,.960),(.552,.938),(.577,.927),
            (.601,.907),(.624,.895),(.650,.870),(.673,.854),(.700,.838),
            (.727,.825),(.753,.816),(.778,.807),(.803,.802),(.830,.800),
            (.855,.795),(.882,.792),(.902,.779),(.918,.765),(.932,.742),
            (.946,.718),(.961,.705),(.975,.681),(.990,.663),(1,.657),(1,1)]
    else:
        points=[(.708,1),(.727,.953),(.745,.915),(.756,.863),(.768,.827),
            (.779,.822),(.788,.793),(.785,.762),(.791,.742),(.808,.734),
            (.825,.734),(.831,.755),(.837,.788),(.840,.806),(.848,.801),
            (.857,.801),(.865,.809),(.872,.799),(.883,.811),(.888,.809),
            (.899,.814),(.908,.837),(.920,.842),(.931,.855),(.943,.850),
            (.954,.876),(.971,.884),(.983,.866),(1,.847),(1,1)]
    mask=Image.new('L',(w*scale,h*scale))
    ImageDraw.Draw(mask).polygon([(round(x*w*scale),round(y*h*scale)) for x,y in points],fill=255)
    mask=mask.resize((w,h),Image.Resampling.LANCZOS).filter(ImageFilter.GaussianBlur(.85))
    rgba=rgb.convert('RGBA');rgba.putalpha(mask)
    return rgba


def replace_media(documents,style,fluid_frames=None,field_photos=None):
    if style not in ('chrome','field'):return None
    if style=='chrome':
        if fluid_frames is None:raise ValueError('Chrome rebuild requires --fluid-frames; old letter media must never be restored')
        fluid_frames=Path(fluid_frames)
        for family in ('cover','body'):
            for n in range(48):
                if not (fluid_frames/f'{family}-{n:02}.png').is_file():
                    raise ValueError(f'Missing original fluid frame {family}-{n:02}.png')
        source_backgrounds={}
    else:
        if field_photos is None:raise ValueError('Field rebuild requires --field-photos; old photos/mattes must never be restored')
        field_photos=Path(field_photos)
        photo_manifest=json.loads((Path(__file__).parent/'pptx-template-photo-sources.json').read_text())
        photo_info={x['file']:x for x in photo_manifest['sources']}
        for filename in photo_info:
            if not (field_photos/filename).is_file():raise ValueError(f'Missing public-domain photo {filename}')
            photo_info[filename]['importSha256']=hashlib.sha256((field_photos/filename).read_bytes()).hexdigest()
    replaced=0;unique=set();cover_objects=0;body_objects=0
    for slide in range(1,11):
        name=f'ppt/slides/slide{slide}.xml';relname=f'ppt/slides/_rels/slide{slide}.xml.rels'
        doc=minidom.parseString(documents[name]);rels=minidom.parseString(documents[relname])
        frames={'cover':0,'body':0};photo_count=0
        for pic in list(doc.getElementsByTagName('p:pic')):
            name_label=label(pic);parent=pic.parentNode;ancestor=''
            while parent is not None and parent.nodeType==Node.ELEMENT_NODE:
                if parent.tagName=='p:grpSp':ancestor=label(parent);break
                parent=parent.parentNode
            blip=first(pic,'a:blip')
            if blip is None:continue
            if style=='chrome':
                if '黑场' in name_label:
                    # The user requested replacement of letter media only.
                    # Retain the non-letter blue/black refraction background.
                    original=next(r for r in rels.getElementsByTagName('Relationship')
                        if r.getAttribute('Id')==blip.getAttribute('r:embed'))
                    source=posixpath.normpath(posixpath.join('ppt/slides',original.getAttribute('Target')))
                    data=documents[source]
                    source_backgrounds[source]=hashlib.sha256(data).hexdigest()
                    part='ppt/media/fluid-background-'+source_backgrounds[source][:12]+Path(source).suffix.lower()
                elif 'dtid' in ancestor.lower():
                    idx=frames['cover']%48;frames['cover']+=1;cover_objects+=1
                    part=f'ppt/media/fluid-cover-{idx:02}.png';data=(fluid_frames/f'cover-{idx:02}.png').read_bytes()
                elif '形变序列' in ancestor:
                    idx=frames['body']%48;frames['body']+=1;body_objects+=1
                    part=f'ppt/media/fluid-body-{idx:02}.png';data=(fluid_frames/f'body-{idx:02}.png').read_bytes()
                elif 'dtid' in name_label.lower():
                    part='ppt/media/fluid-poster.png';data=(fluid_frames/'cover-00.png').read_bytes()
                else:
                    continue
                # Neutral names also keep the transformed poster in safe_art.
                first(pic,'p:cNvPr').setAttribute('name','黑场 · 原创无字流体')
            else:
                if '证书' in name_label:
                    # Evidence certificates are content, rather than scenery.
                    continue
                is_foreground='建筑或树影前景' in name_label
                is_plate='现场摄影 · 后景' in name_label or is_foreground
                xf=first(pic,'a:xfrm');extent=first(xf,'a:ext')
                width=int(extent.getAttribute('cx'))/12700;height=int(extent.getAttribute('cy'))/12700
                target=(max(32,min(1800,round(width*2))),max(32,min(1800,round(height*2))))
                if is_plate:
                    filename='pacific.jpg' if slide==1 else 'storm.jpg'
                    center_y=.61 if slide==1 else .55
                else:
                    options=['headland.jpg','wildflowers.jpg','storm.jpg','pacific.jpg']
                    filename=options[(slide+photo_count)%len(options)];center_y=.57
                info=photo_info[filename]
                im=cropped_photo(field_photos/filename,width/height,target,info['trim'],center_y)
                if is_foreground:
                    im=coastal_foreground(im,slide);ext='png';data=encode(im)
                else:
                    ext='jpg';data=encode(im,'JPEG')
                ident=first(pic,'p:cNvPr').getAttribute('id')
                part=f'ppt/media/field-photo-{slide}-{ident}.{ext}'
                first(pic,'p:cNvPr').setAttribute('name','现场摄影 · 新海岸前景' if is_foreground else '现场摄影 · NPS海岸')
                photo_count+=1
            documents[part]=data;unique.add(part);replaced+=1
            rid=f'rIdReadMDArt{replaced}'
            relation=rels.createElementNS(REL,'Relationship');relation.setAttribute('Id',rid)
            relation.setAttribute('Type',OFFICE_REL+'/image');relation.setAttribute('Target','../media/'+Path(part).name)
            rels.documentElement.appendChild(relation);blip.setAttribute('r:embed',rid)
            if blip.hasAttribute('r:link'):blip.removeAttribute('r:link')
            for src in list(pic.getElementsByTagName('a:srcRect')):src.parentNode.removeChild(src)
        documents[name]=doc.toxml(encoding='UTF-8',standalone=True)
        documents[relname]=rels.toxml(encoding='UTF-8',standalone=True)
    if style=='chrome':
        return {'design':'original-smooth-letter-free-chrome-fluid','renderer':'Blender Cycles CPU',
            'uniquePoses':96,'coverPictureTargets':cover_objects,'bodyPictureTargets':body_objects,
            'sourceArtMeshesUsed':0,'sourceFluidTexturesUsed':0,'generatedBackground':False,
            'background':'original-blue-black-refraction','sourceBackgroundImagesUsed':len(source_backgrounds),
            'sourceBackgroundSha256':list(source_backgrounds.values()),
            'fluidColorPrecisionBits':6,'fluidAlphaPrecisionBits':8}
    return {'design':'NPS Point Reyes coastal photography','newPhotographObjects':replaced,
        'oldPhotographsUsed':0,'oldForegroundMasksUsed':0,'foregroundMask':'new traced land matte from exact matching photograph crop',
        'photoSources':photo_manifest['sources']}
