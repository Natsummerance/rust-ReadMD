"""Render original, letter-free chrome fluid using Blender's offline CPU Cycles.

blender --background --factory-startup --python tools/render-pptx-fluid.py -- \
  --output <directory> [--count 48] [--samples 24] [--width 800]

No fonts, text objects, source mesh, source texture, AI models or downloads.
Two smoothly looping 48-pose families replace cover and body media. The cover
timeline plays its family twice, retaining the original 96 picture targets.
"""
import argparse
import json
import math
from pathlib import Path
import sys
import time

import bpy
from mathutils import Vector


def aim(obj, point=(0, 0, 0)):
    obj.rotation_euler = (Vector(point) - obj.location).to_track_quat('-Z', 'Y').to_euler()


def softbox(name, location, scale, strength, color):
    bpy.ops.mesh.primitive_plane_add(size=2, location=location)
    obj=bpy.context.object;obj.name=name;obj.scale=scale
    aim(obj)
    material=bpy.data.materials.new(name);material.use_nodes=True
    nodes=material.node_tree.nodes;nodes.clear()
    emission=nodes.new('ShaderNodeEmission');emission.inputs['Color'].default_value=(*color,1)
    emission.inputs['Strength'].default_value=strength
    output=nodes.new('ShaderNodeOutputMaterial');material.node_tree.links.new(emission.outputs[0],output.inputs[0])
    obj.data.materials.append(material)
    obj.visible_camera=False
    return obj


def fluid_vertices(phase, family, segments=128, rings=64):
    vertices=[]
    for j in range(rings+1):
        theta=math.pi*j/rings
        st,ct=math.sin(theta),math.cos(theta)
        for i in range(segments):
            phi=2*math.pi*i/segments
            # A closed genus-zero liquid surface. Unequal smooth lobes have no
            # glyph-like strokes, holes, joints, planes or detached fragments.
            r=1+.17*math.cos(3*phi+.38*math.sin(phase))*st**3
            r+=.085*math.sin(2*theta+phi+phase)*st**2
            r+=.065*math.cos(4*phi-2*theta-phase)*st**3
            x=2.18*r*st*math.cos(phi)
            y=.91*r*st*math.sin(phi)
            z=.92*r*ct
            y+=.22*math.sin(1.38*x+.42*math.sin(phase))*st
            z+=.26*math.sin(1.32*x+.38*math.cos(phase))*st**2
            x+=.10*math.sin(2*z+phase)*st
            if family=='body':
                # Same coherent fluid, a second camera-like pose family.
                y+=.11*math.cos(2*x+phase)*st
                z+=.09*math.sin(3*phi+phase)*st**3
            vertices.append((x,y,z))
    return vertices


def main():
    argsv=sys.argv[sys.argv.index('--')+1:] if '--' in sys.argv else []
    parser=argparse.ArgumentParser()
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--count',type=int,default=48)
    parser.add_argument('--samples',type=int,default=24)
    parser.add_argument('--width',type=int,default=800)
    parser.add_argument('--family',choices=['cover','body','both'],default='both')
    args=parser.parse_args(argsv);args.output.mkdir(parents=True,exist_ok=True)
    bpy.ops.object.select_all(action='SELECT');bpy.ops.object.delete(use_global=False)
    scene=bpy.context.scene;scene.render.engine='CYCLES';scene.cycles.device='CPU'
    scene.cycles.samples=args.samples;scene.cycles.use_denoising=True
    scene.cycles.use_adaptive_sampling=True;scene.cycles.adaptive_threshold=.035
    scene.render.threads_mode='FIXED';scene.render.threads=4
    scene.render.resolution_x=args.width;scene.render.resolution_y=round(args.width*596/1080)
    scene.render.resolution_percentage=100;scene.render.film_transparent=True
    scene.render.image_settings.file_format='PNG';scene.render.image_settings.color_mode='RGBA'
    scene.render.image_settings.color_depth='8';scene.render.image_settings.compression=30
    scene.view_settings.view_transform='AgX';scene.view_settings.look='AgX - Medium High Contrast'
    scene.world.use_nodes=True
    scene.world.node_tree.nodes['Background'].inputs['Color'].default_value=(.07,.085,.13,1)
    scene.world.node_tree.nodes['Background'].inputs['Strength'].default_value=.28
    bpy.ops.object.camera_add(location=(4.7,-8,3.4));camera=bpy.context.object;aim(camera)
    camera.data.type='ORTHO';camera.data.ortho_scale=6.1;scene.camera=camera
    softbox('Studio key',(-3,-4,6),(3.6,2.3,1),4.8,(1,.98,.96))
    softbox('Long cool strip',(2.7,1.5,4),(4.2,.55,1),5.0,(.81,.88,1))
    softbox('White rim',(-4,2,1.0),(2.3,3.5,1),3.8,(.96,.98,1))
    softbox('Lower neutral fill',(1,-3,-3),(4.5,1.6,1),2.0,(.8,.84,.94))
    softbox('Violet reflection',(4.8,1,.3),(1.5,2.2,1),2.4,(.54,.58,1))
    mesh=bpy.data.meshes.new('Original continuous fluid surface')
    segments,rings=128,64;verts=fluid_vertices(0,'cover',segments,rings)
    faces=[]
    for j in range(rings):
        for i in range(segments):
            a=j*segments+i;b=j*segments+(i+1)%segments
            faces.append((a,b,b+segments,a+segments))
    mesh.from_pydata(verts,[],faces);mesh.update()
    fluid=bpy.data.objects.new('Letter-free chrome fluid',mesh);scene.collection.objects.link(fluid)
    for polygon in mesh.polygons:polygon.use_smooth=True
    material=bpy.data.materials.new('Neutral polished chromium');material.use_nodes=True
    bsdf=material.node_tree.nodes.get('Principled BSDF')
    bsdf.inputs['Base Color'].default_value=(.72,.75,.81,1)
    bsdf.inputs['Metallic'].default_value=1;bsdf.inputs['Roughness'].default_value=.105
    fluid.data.materials.append(material)
    families=['cover','body'] if args.family=='both' else [args.family]
    began=time.time()
    for family in families:
        for frame in range(args.count):
            output=args.output/f'{family}-{frame:02}.png'
            if output.exists():continue
            phase=2*math.pi*frame/48
            for v,position in zip(mesh.vertices,fluid_vertices(phase,family,segments,rings)):
                v.co=position
            mesh.update()
            fluid.rotation_euler=(.12*math.sin(phase),.15*math.cos(phase),.08*math.sin(phase))
            if family=='body':fluid.rotation_euler.z+=.42
            scene.render.filepath=str(output)
            bpy.ops.render.render(write_still=True)
            print(f'FLUID {family} {frame+1}/{args.count}; elapsed {time.time()-began:.1f}s',flush=True)
    (args.output/'manifest.json').write_text(json.dumps({
        'schema':1,'design':'original-smooth-letter-free-chrome-fluid','renderer':'Blender Cycles CPU',
        'width':scene.render.resolution_x,'height':scene.render.resolution_y,'samples':args.samples,
        'families':['cover','body'],'posesPerFamily':48,'sourceTextObjects':0,'sourceMeshes':0,
        'timeline':'cover uses 48 unique poses twice across 96 existing picture targets'
    },separators=(',',':'))+'\n')


if __name__=='__main__':main()
