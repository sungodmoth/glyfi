import json
import subprocess
import glob

def latex_escape(string):
    ## Escapes strings so that they can be used in LaTeX. Currently only escapes _,
    ## because the others seem to cause problems in filenames even if escaped.
    #group1 = '&%$#_{}'
    group1 = '_'
    #group2 = '~^'
    for char in group1:
        string = string.replace(char, '\\'+char)
    #for char in group2:
    #    string = string.replace(char, '\\'+char+'{}')
    return string
def determine_columns(n, min_, max_=None):
    ## Determines the number of columns in which n submissions ought to be displayed.
    ## Maximally square but without being more columns than rows or fewer than min_ columns.
    for i in range(min_,n):
        if -(n//-(i+1)) < i + 1:
            return i if not max_ else min(i, max_)
    return min_

def font_size_format(fontname, size):
    ## Outputs the required LaTeX string to set a given font with a given size,
    ## including the case in which we don't specify either.
    buf = ""
    if size:
        buf = fr"\fontsize{{{size}}}{{{size}}}\selectfont " + buf
    if fontname:
        buf = fr"\setmainfont{{{fontname}}}" + buf
    return buf

def match_and_format_font(string, fonts, font_override, size_override, default_size, verbose):
    ## Combines font_size_format and match_font to automatically find the correct
    ## font for a glyph and output the correct LaTeX sequence to display it.
    size = size_override
    autofont = {'name': None}
    if not font_override:
        autofont = match_font(string, fonts)
        if not autofont:
            autofont = {'name': None}
        #for some fonts, we may want to automatically scale them
        if "size_percentage" in autofont:
            size = size_override or round((int(autofont["size_percentage"]) * default_size)/100)
        #we may want to load a font in a special way (e.g. particular options)
        if "load_as" in autofont:
            return autofont["load_as"] + font_size_format(None, size) + string
    fontname = font_override or autofont['name']
    if verbose:
        print(f"{fontname} used for string {string}.")
    return font_size_format(fontname, size) + string
    
def get_ranges(fontname):
    ## Given a font name, uses fontconfig to determine which glyph ranges it supports.
    if fontname == "STIXTwoText":
        #this case is hardcoded because STIX might not be present on the system as a ttf/otf
        return [(32, 126), (160, 384), (392, 392), (400, 400), (402, 402), (405, 405), (409, 411), (414, 414), (416, 417), (421, 421), (426, 427), (429, 429), (431, 432), (437, 437), (442, 443), (446, 446), (448, 451), (478, 479), (496, 496), (506, 511), (536, 539), (545, 545), (552, 553), (564, 567), (592, 745), (748, 749), (759, 759), (768, 831), (838, 839), (844, 844), (857, 857), (860, 860), (864, 866), (894, 894), (900, 906), (908, 908), (910, 929), (931, 974), (976, 978), (981, 982), (984, 993), (1008, 1009), (1012, 1014), (1024, 1119), (1122, 1123), (1130, 1131), (1138, 1141), (1168, 1169), (7424, 7424), (7431, 7431), (7452, 7452), (7553, 7553), (7556, 7557), (7562, 7562), (7565, 7566), (7576, 7576), (7587, 7587), (7680, 7929), (8192, 8205), (8208, 8226), (8229, 8230), (8239, 8252), (8254, 8254), (8256, 8256), (8259, 8260), (8263, 8263), (8267, 8274), (8279, 8279), (8287, 8287), (8304, 8305), (8308, 8334), (8355, 8356), (8359, 8359), (8363, 8364), (8377, 8378), (8381, 8381), (8400, 8402), (8406, 8407), (8411, 8415), (8417, 8417), (8420, 8432), (8448, 8527), (8531, 8542), (8722, 8722), (8725, 8725), (9251, 9251), (9676, 9676), (42791, 42791), (42898, 42898), (64256, 64260)]
    process = subprocess.run(["fc-match", "--format='%{charset}\\n", fontname], capture_output=True)
    ranges = []
    for x in str(process.stdout)[3:][:-3].split(" "):
        a, *b = x.split("-")
        b = (b or [a])[0]
        ranges.append((int(a, 16), int(b, 16)))
    return ranges

def get_all_ranges(fontdata):
    ## Applies the above to all fonts in a json list, as parsed from fontdata.json
    output = []
    for font in fontdata:
        output.append(get_ranges(font['name']))
    return output

def parse_fontdata():
    ## Parses font information from fontdata.json.
    with open("fontdata.json", "r") as f:
        return json.loads(f.read())["fonts"]

def number_of_submissions(prefix):
    ## Determines how many submissions there are by looking for files with the prefix and a number.
    i = 1
    while True:
        if glob.glob(f"Images/{prefix}_{i}.*"):
            i += 1
        else:
            return i - 1

def match_font(string, fonts):
    ## Given a string and a list of fonts, in the format parsed from
    ## fontdata.json, finds the first font in the list which supports
    ## (and does not exclude) all of the characters in the string.
    for (font, ranges) in fonts:
        for char in string:
            match = False
            for rnge in ranges:
                if rnge[0] <= ord(char) <= rnge[1]:
                    match = True
                    break
            if match == False:
                break
            if "excludes" in font:
                for rule in font["excludes"]:
                    start, end = map(lambda x: int(x, 16), rule.split("-"))
                    if start <= ord(char) <= end:
                        match = False
                if match == False:
                    break
        if match == True:
            return font

def run_subprocess(process, verbose):
    ## Runs a subprocess to completion, printing its output only if verbose.
    return_code = None
    while True:
        output = process.stdout.readline()
        if verbose:
            print(output.strip())
        return_code = process.poll()
        if return_code is not None:
            #process has finished
            if verbose:
                #read the rest of the output 
                for output in process.stdout.readlines():
                    print(output.strip())
            return return_code