#!/usr/bin/env python3
"""Package an official llama.cpp build as relocatable, signed app resources."""
import pathlib, shutil, subprocess, sys, os
project = pathlib.Path(__file__).resolve().parent.parent
source = pathlib.Path(sys.argv[1]).resolve()
target = project / 'Resources' / 'Runtime'
target.mkdir(parents=True, exist_ok=True)
for binary in [source / 'llama-server', *source.glob('*.dylib')]:
    destination = target / binary.name
    if destination.exists() or destination.is_symlink(): destination.unlink()
    if binary.is_symlink(): destination.symlink_to(os.readlink(binary))
    else: shutil.copy2(binary, destination)
for binary in target.iterdir():
    if binary.is_symlink() or (binary.name != 'llama-server' and binary.suffix != '.dylib'): continue
    lines = subprocess.check_output(['otool','-L',str(binary)],text=True).splitlines()[1:]
    for line in lines:
        dependency = line.strip().split(' (',1)[0]
        basename = pathlib.Path(dependency).name
        if (target / basename).exists():
            subprocess.run(['install_name_tool','-change',dependency,'@loader_path/'+basename,str(binary)],check=True)
    load_commands = subprocess.check_output(['otool','-l',str(binary)],text=True).splitlines()
    for index,line in enumerate(load_commands):
        if line.strip() == 'cmd LC_RPATH':
            path = load_commands[index+2].strip().split('path ',1)[1].split(' (offset',1)[0]
            if path.startswith('/'):
                subprocess.run(['install_name_tool','-delete_rpath',path,str(binary)],check=True)
    if binary.suffix == '.dylib':
        subprocess.run(['install_name_tool','-id','@rpath/'+binary.name,str(binary)],check=True)
    subprocess.run(['codesign','--force','--sign','-',str(binary)],check=True)
print(target)
