#!/usr/bin/python
import subprocess
import sys, os
import argparse
import datetime
import time
import zoneinfo
#local
from utility import *

#constants, could be made arguments later if needed
RENDER_DPI = 700
DOWNSCALE_PERCENTAGE = 50

def extract_from_pdf(pdf_filename, output_filename, render_dpi, downscale_percentage, verbose):
    ## Extracts a single image from an outputted pdf. We use both pdftoppm and imagemagick for this,
    ## first rendering at a high dpi and then downscaling to ensure high quality rasterisation.
    ##################################RASTERISE############################################
    print("Rendering image with pdftoppm...")
    process = subprocess.Popen(["pdftoppm", "-png", "-singlefile", "-r", f"{render_dpi}", 
                                "-aaVector", "no", f"{pdf_filename}", f"{output_filename}"],
                                stdout=subprocess.PIPE,
                                stderr=subprocess.PIPE,
                                universal_newlines=True)
    pdftoppm_return = run_subprocess(process, verbose)
    if pdftoppm_return != 0:
        print("pdftoppm exited with an error, exiting...")
        return pdftoppm_return
    ##################################DOWNSCALE############################################
    print("Downscaling with imagemagick...")
    process = subprocess.Popen(["convert", f"{output_filename}.png", "-resize", 
                                f"{downscale_percentage}%", f"{output_filename}.png"],
                                stdout=subprocess.PIPE,
                                stderr=subprocess.PIPE,
                                universal_newlines=True)
    imagemagick_return = run_subprocess(process, verbose)
    if imagemagick_return != 0:
        print("imagemagick exited with an error, exiting...")
        return imagemagick_return
    #######################################################################################
    return 0

if __name__ == "__main__":
    ##################################ARGPARSE#############################################
    parser = argparse.ArgumentParser(description="Compiles a single glyph/ambigram challenge image and outputs as png. Requires LaTeX installation, pdftoppm and imagemagick.")
    parser.add_argument("-v", "--verbose", action="store_true", help="print outputs of the subprocesses (e.g. pdftoppm) - primarily useful for debugging")
    parser.add_argument("-o", "--out", type=str, default=None, help="name of the output png (if unspecified, will follow the name of the chosen subcommand e.g. glyph_announcement.png)", metavar="FILE")
    parser.add_argument("--start_date", type=str, default=None, help="date of beginning of challenge")
    parser.add_argument("--end_date", type=str, default=None, help="date of end of challenge")
    parser.add_argument("--week", type=int, default=None, help="current week number")
    subcommands = parser.add_subparsers(title="subcommands", description="run ``<SUBCOMMAND> --help`` for that subcommand's usage", required=True, dest="subcommand")
    glyph_announcement = subcommands.add_parser("glyph_announcement", help="glyph_announcement [-size_percentage PERCENT] <GLYPH>")
    glyph_announcement.add_argument("glyph", help="the glyph to be announced")
    glyph_announcement.add_argument("--size_percentage", type=int, default=None, help="percentage modifier to be applied to the font size")
    ambigram_announcement = subcommands.add_parser("ambigram_announcement", help="ambigram_announcement [-size_percentage PERCENT] <AMBI>")
    ambigram_announcement.add_argument("ambi")
    ambigram_announcement.add_argument("--size_percentage", type=int, default=None, help="percentage modifier to be applied to the font size")
    glyph_poll = subcommands.add_parser("glyph_poll", help="glyph_poll [--cols N] [-size_percentage PERCENT] <GLYPH>")
    glyph_poll.add_argument("glyph")
    glyph_poll.add_argument("--size_percentage", type=int, default=None, help="percentage modifier to be applied to the font size")
    glyph_poll.add_argument("--cols", type=int, default=None, help="width in columns (determined from number of submissions by default)")
    ambigram_poll = subcommands.add_parser("ambigram_poll", help="ambigram_poll [--cols N] [-size_percentage PERCENT] <AMBI>")
    ambigram_poll.add_argument("ambi")
    ambigram_poll.add_argument("--size_percentage", type=int, default=None, help="percentage modifier to be applied to the font size")
    ambigram_poll.add_argument("--cols", type=int, default=None, help="width in columns (determined from number of submissions by default)")
    glyph_first = subcommands.add_parser("glyph_first", help="glyph_first <NICKNAME> <USER_ID> <SUB_ID> [-size_percentage PERCENT]")
    glyph_first.add_argument("nickname")
    glyph_first.add_argument("user_id")
    glyph_first.add_argument("sub_id")
    glyph_first.add_argument("--size_percentage", type=int, default=None, help="percentage modifier to be applied to the font size")
    glyph_second = subcommands.add_parser("glyph_second", help="glyph_second <NICKNAME> <USER_ID> <SUB_ID> [-size_percentage PERCENT]")
    glyph_second.add_argument("nickname")
    glyph_second.add_argument("user_id")
    glyph_second.add_argument("sub_id")
    glyph_second.add_argument("--size_percentage", type=int, default=None, help="percentage modifier to be applied to the font size")
    glyph_third = subcommands.add_parser("glyph_third", help="glyph_third <NICKNAME> <USER_ID> <SUB_ID> [-size_percentage PERCENT]")
    glyph_third.add_argument("nickname")
    glyph_third.add_argument("user_id")
    glyph_third.add_argument("sub_id")
    glyph_third.add_argument("--size_percentage", type=int, default=None, help="percentage modifier to be applied to the font size")
    ambigram_first = subcommands.add_parser("ambigram_first", help="ambigram_first <NICKNAME> <USER_ID> <SUB_ID> [-size_percentage PERCENT]")
    ambigram_first.add_argument("nickname")
    ambigram_first.add_argument("user_id")
    ambigram_first.add_argument("sub_id")
    ambigram_first.add_argument("--size_percentage", type=int, default=None, help="percentage modifier to be applied to the font size")
    ambigram_second = subcommands.add_parser("ambigram_second", help="ambigram_second <NICKNAME> <USER_ID> <SUB_ID> [-size_percentage PERCENT]")
    ambigram_second.add_argument("nickname")
    ambigram_second.add_argument("user_id")
    ambigram_second.add_argument("sub_id")
    ambigram_second.add_argument("--size_percentage", type=int, default=None, help="percentage modifier to be applied to the font size")
    ambigram_third = subcommands.add_parser("ambigram_third", help="ambigram_third <NICKNAME> <USER_ID> <SUB_ID> [-size_percentage PERCENT]")
    ambigram_third.add_argument("nickname")
    ambigram_third.add_argument("user_id")
    ambigram_third.add_argument("sub_id")
    ambigram_third.add_argument("--size_percentage", type=int, default=None, help="percentage modifier to be applied to the font size")
    glyph_suggestions = subcommands.add_parser("glyph_suggestions", help="glyph_suggestions [--cols N] <GLYPH1> <GLYPH2> [...]")
    glyph_suggestions.add_argument("glyphs", nargs='*')
    glyph_suggestions.add_argument("--cols", type=int, default=None, help="width in columns (determined from number of suggestions by default)")
    ambigram_suggestions = subcommands.add_parser("ambigram_suggestions", help="ambigram_suggestions [--cols N] <AMBI1> <AMBI2> [...]")
    ambigram_suggestions.add_argument("ambis", nargs='*')
    ambigram_suggestions.add_argument("--cols", type=int, default=None, help="width in columns (determined from number of suggestions by default)")



    args = parser.parse_args()
    ##################################INJECTION############################################
    fontdata = parse_fontdata()
    scripts = parse_scriptdata()
    fonts = list(zip(fontdata, get_all_ranges(fontdata)))
    #########################DATE###############################
    #current date in europe timezone
    cycle_number = args.week
    if cycle_number == 0:
        week_colour = "Blue"
    elif cycle_number == 1:
        week_colour = "Pink"
    elif cycle_number == 2:
        week_colour = "Cyan"
    else:
        week_colour = "Red"
    date_formatted = args.start_date
    ########################FILE STUFF##########################
    with open("weekly_challenges_base.tex", "r", encoding='utf8') as f:
        contents = f.read()
    with open("weekly_challenges.tex", "w", encoding='utf8') as f:
        f.write(contents)
        f.writelines(
fr"""
\SetDate[{args.start_date}]
\SaveDate[\StartDate]
\SetDate[{args.end_date}]
\SaveDate[\EndDate]
\WeekColor{{{week_colour}}}
""")
        ####################GLYPH_ANNOUNCEMENT##################
        if args.subcommand == "glyph_announcement":
            glyph_formatted = match_and_format_font(args.glyph, fonts, scripts, args.size_percentage, 100, "", args.verbose)
            f.writelines(
fr"""
\def\NextWeekGlyph{{{glyph_formatted}}}
\begin{{document}}
\GlyphChallengeAnnouncement
\end{{document}}
""")
        ####################AMBIGRAM_ANNOUNCEMENT###############
        if args.subcommand == "ambigram_announcement":
            ambi_formatted = match_and_format_font(args.ambi, fonts, scripts, args.size_percentage, 80, "", args.verbose)
            f.writelines(
fr"""
\def\NextWeekAmbigram{{{ambi_formatted}}}
\begin{{document}}
\AmbigramChallengeAnnouncement
\end{{document}}
""")
        ####################GLYPH_POLL########################
        if args.subcommand == "glyph_poll":
            dirr = f"images/glyph/{args.week}"
            subs = sorted(list(filter(lambda x: os.path.isfile(f"{dirr}/{x}"), os.listdir(dirr))))
            buf = ""
            i = 1
            for sub in subs:
                path = f"{dirr}/{sub.split('.')[0]}"
                buf += fr"""\setimage{{{i}}}{{{path}}}
"""
                i += 1
            f.writelines(
fr"""
\def\ThisWeekGlyph{{{match_and_format_font(args.glyph, fonts, scripts, args.size_percentage, 60, "", args.verbose)}}}
\begin{{document}}
\def\NumberOfSubs{{{len(subs)}}}
{buf}
\glyphlabels
\GlyphChallengeShowcase{{9}}{{{args.cols or determine_columns(len(subs),3, max_=4)}}}
\end{{document}}
""")
        ####################AMBIGRAM_POLL#####################
        if args.subcommand == "ambigram_poll":
            dirr = f"images/ambi/{args.week}"
            subs = sorted(list(filter(lambda x: os.path.isfile(f"{dirr}/{x}"), os.listdir(dirr))))
            buf = ""
            i = 1
            for sub in subs:
                path = f"{dirr}/{sub.split('.')[0]}"
                buf += fr"""\setimage{{{i}}}{{{path}}}
"""
                i += 1
            f.writelines(
fr"""
\def\ThisWeekAmbigram{{{match_and_format_font(args.ambi, fonts, scripts, args.size_percentage, 22, "", args.verbose)}}}
\begin{{document}}
\def\NumberOfAmbis{{{len(subs)}}}
{buf}
\glyphlabels
\AmbigramChallengeShowcase{{11}}{{{args.cols or determine_columns(len(subs), 3, max_=3)}}}
\end{{document}}
""")
        ####################GLYPH_WINNERS########################
        if args.subcommand == "glyph_first":
            style = "\\itshape\\bfseries"
            f.writelines(
fr"""
\def\GlyphWinnerFirst{{{match_and_format_font(latex_escape(args.nickname), fonts, scripts, args.size_percentage, 40, style, args.verbose)}}}
\def\GlyphWinnerFirstID{{{latex_escape(args.user_id)}}}
\def\GlyphWinnerFirstSubID{{{latex_escape(args.sub_id)}}}
\def\WeekNum{{{args.week}}}
\begin{{document}}
\GlyphChallengeFirst
\end{{document}}
""")
        if args.subcommand == "glyph_second":
            style = "\\itshape\\bfseries"
            f.writelines(
fr"""
\def\GlyphWinnerSecond{{{match_and_format_font(latex_escape(args.nickname), fonts, scripts, args.size_percentage, 40, style, args.verbose)}}}
\def\GlyphWinnerSecondID{{{latex_escape(args.user_id)}}}
\def\GlyphWinnerSecondSubID{{{latex_escape(args.sub_id)}}}
\def\WeekNum{{{args.week}}}
\begin{{document}}
\GlyphChallengeSecond
\end{{document}}
""")
        if args.subcommand == "glyph_third":
            style = "\\itshape\\bfseries"
            f.writelines(
fr"""
\def\GlyphWinnerThird{{{match_and_format_font(latex_escape(args.nickname), fonts, scripts, args.size_percentage, 40, style, args.verbose)}}}
\def\GlyphWinnerThirdID{{{latex_escape(args.user_id)}}}
\def\GlyphWinnerThirdSubID{{{latex_escape(args.sub_id)}}}
\def\WeekNum{{{args.week}}}
\begin{{document}}
\GlyphChallengeThird
\end{{document}}
""")
        ####################AMBIGRAM_WINNERS#####################
        if args.subcommand == "ambigram_first":
            style = "\\itshape\\bfseries"
            f.writelines(
fr"""
\def\AmbiWinnerFirst{{{match_and_format_font(latex_escape(args.nickname), fonts, scripts, args.size_percentage, 40, style, args.verbose)}}}
\def\AmbiWinnerFirstID{{{latex_escape(args.user_id)}}}
\def\AmbiWinnerFirstSubID{{{latex_escape(args.sub_id)}}}
\def\WeekNum{{{args.week}}}
\begin{{document}}
\AmbigramChallengeFirst
\end{{document}}
""")
        if args.subcommand == "ambigram_second":
            style = "\\itshape\\bfseries"
            f.writelines(
fr"""
\def\AmbiWinnerSecond{{{match_and_format_font(latex_escape(args.nickname), fonts, scripts, args.size_percentage, 40, style, args.verbose)}}}
\def\AmbiWinnerSecondID{{{latex_escape(args.user_id)}}}
\def\AmbiWinnerSecondSubID{{{latex_escape(args.sub_id)}}}
\def\WeekNum{{{args.week}}}
\begin{{document}}
\AmbigramChallengeSecond
\end{{document}}
""")
        if args.subcommand == "ambigram_third":
            style = "\\itshape\\bfseries"
            f.writelines(
fr"""
\def\AmbiWinnerThird{{{match_and_format_font(latex_escape(args.nickname), fonts, scripts, args.size_percentage, 40, style, args.verbose)}}}
\def\AmbiWinnerThirdID{{{latex_escape(args.user_id)}}}
\def\AmbiWinnerThirdSubID{{{latex_escape(args.sub_id)}}}
\def\WeekNum{{{args.week}}}
\begin{{document}}
\AmbigramChallengeThird
\end{{document}}
""")
        ####################GLYPH_SUGGESTIONS##########################
        if args.subcommand == "glyph_suggestions":
            suggestions_formatted = ""
            i = 0
            if args.glyphs:
                for glyph in args.glyphs:
                    i += 1
                    suggestions_formatted += fr"""\setpollglyph{{{i}}}{{{match_and_format_font(glyph, fonts, scripts, None, 40, "", args.verbose)}}}
    """
            else:
                print("No arguments given; taking suggestions from glyph_suggestions.txt...")
                with open("glyph_suggestions.txt", "r") as g:
                    lines = g.readlines()
                for line in lines:
                    a = line.strip().split("\t")[::-1]
                    glyph = (a or [None]).pop()
                    size_override = (a or [None]).pop()
                    if glyph:
                        i += 1
                        suggestions_formatted += fr"""\setpollglyph{{{i}}}{{{match_and_format_font(glyph, fonts, scripts, size_override, 40, "", args.verbose)}}}
    """
            f.writelines(
fr"""
\def\GlyphSuggestions{{{i}}}
\def\pollglyphs{{
    {suggestions_formatted}
}}
\begin{{document}}
\glyphlabels
\GlyphPoll{{{args.cols or determine_columns(i,4)}}}
\end{{document}}
""")
        ####################AMBIGRAM_SUGGESTIONS#######################
        if args.subcommand == "ambigram_suggestions":
            suggestions_formatted = ""
            i = 0
            if args.ambis:
                for ambi in args.ambis:
                    i += 1
                    suggestions_formatted += fr"""\setpollambi{{{i}}}{{{match_and_format_font(ambi, fonts, scripts, None, 28, "", args.verbose)}}}
    """
            else:
                print("No arguments given; taking suggestions from ambigram_suggestions.txt...")
                with open("ambigram_suggestions.txt", "r") as g:
                    lines = g.readlines()
                for line in lines:
                    a = line.strip().split("\t")[::-1]
                    ambi = (a or [None]).pop()
                    size_override = (a or [None]).pop()
                    if ambi:
                        i += 1
                        suggestions_formatted += fr"""\setpollambi{{{i}}}{{{match_and_format_font(ambi, fonts, scripts, size_override, 28, "", args.verbose)}}}
    """
            f.writelines(
fr"""
\def\AmbiSuggestions{{{i}}}
\def\pollambigrams{{
    {suggestions_formatted}
}}
\begin{{document}}
\glyphlabels
\AmbigramPoll{{{args.cols or determine_columns(i,3, max_=3)}}}
\end{{document}}
""")
    ##################################COMPILATION##########################################
    print("Compiling LaTeX code...")
    process = subprocess.Popen(["xelatex", "-interaction=nonstopmode", "weekly_challenges.tex"],
                                stdout=subprocess.PIPE,
                                stderr=subprocess.PIPE,
                                universal_newlines=True)
    latex_return = run_subprocess(process, args.verbose)
    if latex_return != 0:
        print("LaTeX exited with an error, exiting...")
        sys.exit(latex_return)
    sys.exit(extract_from_pdf("weekly_challenges.pdf", args.out or args.subcommand, RENDER_DPI, DOWNSCALE_PERCENTAGE, args.verbose))
