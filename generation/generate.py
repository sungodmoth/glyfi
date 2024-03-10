#!/usr/bin/python
import subprocess
import sys
import argparse
import datetime
import time
import zoneinfo
#local
from utility import *

#constants, could be made arguments later if needed
RENDER_DPI = 1000
TARGET_DPI = 500

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
    parser.add_argument("-d", "--date", type=str, default=None, help="date to be displayed on generated images (format DD/MM/YYYY) - defaults to today's date, but override may be needed if we are late")
    subcommands = parser.add_subparsers(title="subcommands", description="run ``<SUBCOMMAND> --help`` for that subcommand's usage", required=True, dest="subcommand")
    glyph_announcement = subcommands.add_parser("glyph_announcement", help="glyph_announcement [-font FONT] [-size SIZE] <GLYPH>")
    glyph_announcement.add_argument("glyph", help="the glyph to be announced")
    glyph_announcement.add_argument("--font", type=str, default=None, help="the font to be used; normally this is determined automatically, but we may want to override it")
    glyph_announcement.add_argument("--size", type=int, default=None, help="the font size to be used; usually default (100) is fine but may need to be overriden if too big/small")
    ambigram_announcement = subcommands.add_parser("ambigram_announcement", help="ambigram_announcement [-font FONT] [-size SIZE] <AMBI>")
    ambigram_announcement.add_argument("ambi")
    ambigram_announcement.add_argument("--font", type=str, default=None, help="the font to be used; normally this is determined automatically, but we may want to override it")
    ambigram_announcement.add_argument("--size", type=int, default=None, help="the font size to be used; usually default (80) is fine but may need to be overriden if too big/small")
    glyph_poll = subcommands.add_parser("glyph_poll", help="glyph_poll [--cols N]  [-font FONT] [-size SIZE] <GLYPH>")
    glyph_poll.add_argument("glyph")
    glyph_poll.add_argument("--font", type=str, default=None, help="the font to be used; normally this is determined automatically, but we may want to override it")
    glyph_poll.add_argument("--size", type=int, default=None, help="the font size to be used; usually default (60) is fine but may need to be overriden if too big/small")
    glyph_poll.add_argument("--cols", type=int, default=None, help="width in columns (determined from number of submissions by default)")
    ambigram_poll = subcommands.add_parser("ambigram_poll", help="ambigram_poll [--cols N]  [-font FONT] [-size SIZE] <AMBI>")
    ambigram_poll.add_argument("ambi")
    ambigram_poll.add_argument("--font", type=str, default=None, help="the font to be used; normally this is determined automatically, but we may want to override it")
    ambigram_poll.add_argument("--size", type=int, default=None, help="the font size to be used; usually default (22) is fine but may need to be overriden if too big/small")
    ambigram_poll.add_argument("--cols", type=int, default=None, help="width in columns (determined from number of submissions by default)")
    glyph_first = subcommands.add_parser("glyph_first", help="glyph_first <WINNER>")
    glyph_first.add_argument("winner")
    glyph_second = subcommands.add_parser("glyph_second", help="glyph_second <WINNER>")
    glyph_second.add_argument("winner")
    glyph_third = subcommands.add_parser("glyph_third", help="glyph_third <WINNER>")
    glyph_third.add_argument("winner")
    ambigram_first = subcommands.add_parser("ambigram_first", help="ambigram_first <WINNER>")
    ambigram_first.add_argument("winner")
    ambigram_second = subcommands.add_parser("ambigram_second", help="ambigram_second <WINNER>")
    ambigram_second.add_argument("winner")
    ambigram_third = subcommands.add_parser("ambigram_third", help="ambigram_third <WINNER>")
    ambigram_third.add_argument("winner")
    glyph_suggestions = subcommands.add_parser("glyph_suggestions", help="glyph_suggestions [--cols N] <GLYPH1> <GLYPH2> [...]")
    glyph_suggestions.add_argument("glyphs", nargs='*')
    glyph_suggestions.add_argument("--cols", type=int, default=None, help="width in columns (determined from number of suggestions by default)")
    ambigram_suggestions = subcommands.add_parser("ambigram_suggestions", help="ambigram_suggestions [--cols N] <AMBI1> <AMBI2> [...]")
    ambigram_suggestions.add_argument("ambis", nargs='*')
    ambigram_suggestions.add_argument("--cols", type=int, default=None, help="width in columns (determined from number of suggestions by default)")



    args = parser.parse_args()
    ##################################INJECTION############################################
    fontdata = parse_fontdata()
    fonts = list(zip(fontdata, get_all_ranges(fontdata)))
    #########################DATE###############################
    #current date in europe timezone
    date = datetime.datetime.now(tz=zoneinfo.ZoneInfo("Europe/Amsterdam")).date()
    #unix timestamp, but taking 4th January 1970 as our epoch, because that's a Sunday
    date_unix_sunday = int(time.mktime(date.timetuple())) - 259200
    #how many weeks have passed since epoch, where weeks are counted as starting on Sunday
    week_number = date_unix_sunday // 604800
    #now modulo this to get the four-week colour cycle
    cycle_number = week_number % 4
    if cycle_number == 0:
        week_colour = "Blue"
    elif cycle_number == 1:
        week_colour = "Pink"
    elif cycle_number == 2:
        week_colour = "Cyan"
    else:
        week_colour = "Red"
    date_formatted = args.date or date.strftime("%d/%m/%Y")
    ########################FILE STUFF##########################
    with open("weekly_challenges_base.tex", "r", encoding='utf8') as f:
        contents = f.read()
    with open("weekly_challenges.tex", "w", encoding='utf8') as f:
        f.write(contents)
        f.writelines(
fr"""
\SetDate[{date_formatted}]
\SaveDate[\Today]
\AdvanceDate[7]
\SaveDate[\NextWeek]
\AdvanceDate[-14]
\SaveDate[\ThisWeek]
\AdvanceDate[-7]
\SaveDate[\LastWeek]
\SetDate[\Today]
\WeekColor{{{week_colour}}}
""")
        ####################GLYPH_ANNOUNCEMENT##################
        if args.subcommand == "glyph_announcement":
            f.writelines(
fr"""
\def\NextWeekGlyph{{{match_and_format_font(args.glyph, fonts, args.font, args.size, 100, args.verbose)}}}
\begin{{document}}
\GlyphChallengeAnnouncement
\end{{document}}
""")
        ####################AMBIGRAM_ANNOUNCEMENT###############
        if args.subcommand == "ambigram_announcement":
            f.writelines(
fr"""
\def\NextWeekAmbigram{{{match_and_format_font(args.ambi, fonts, args.font, args.size, 80, args.verbose)}}}
\begin{{document}}
\AmbigramChallengeAnnouncement
\end{{document}}
""")
        ####################GLYPH_POLL########################
        if args.subcommand == "glyph_poll":
            numsubs = number_of_submissions("Glyph")
            f.writelines(
fr"""
\def\ThisWeekGlyph{{{match_and_format_font(args.glyph, fonts, args.font, args.size, 60, args.verbose)}}}
\begin{{document}}
\def\NumberOfSubs{{{numsubs}}}
\glyphlabels
\GlyphChallengeShowcase{{9}}{{{args.cols or determine_columns(numsubs,3)}}}
\end{{document}}
""")
        ####################AMBIGRAM_POLL#####################
        if args.subcommand == "ambigram_poll":
            numsubs = number_of_submissions("Ambi")
            f.writelines(
fr"""
\def\ThisWeekAmbigram{{{match_and_format_font(args.ambi, fonts, args.font, args.size, args.verbose, 22)}}}
\begin{{document}}
\def\NumberOfAmbis{{{numsubs}}}
\glyphlabels
\AmbigramChallengeShowcase{{11}}{{{args.cols or determine_columns(numsubs,3)}}}
\end{{document}}
""")
        ####################GLYPH_WINNERS########################
        if args.subcommand == "glyph_first":
            f.writelines(
fr"""
\def\GlyphWinnerFirst{{{latex_escape(args.winner)}}}
\begin{{document}}
\GlyphChallengeFirst
\end{{document}}
""")
        if args.subcommand == "glyph_second":
            f.writelines(
fr"""
\def\GlyphWinnerSecond{{{latex_escape(args.winner)}}}
\begin{{document}}
\GlyphChallengeSecond
\end{{document}}
""")
        if args.subcommand == "glyph_third":
            f.writelines(
fr"""
\def\GlyphWinnerThird{{{latex_escape(args.winner)}}}
\begin{{document}}
\GlyphChallengeThird
\end{{document}}
""")
        ####################AMBIGRAM_WINNERS#####################
        if args.subcommand == "ambigram_first":
            f.writelines(
fr"""
\def\AmbiWinnerFirst{{{latex_escape(args.winner)}}}
\begin{{document}}
\AmbigramChallengeFirst
\end{{document}}
""")
        if args.subcommand == "ambigram_second":
            f.writelines(
fr"""
\def\AmbiWinnerSecond{{{latex_escape(args.winner)}}}
\begin{{document}}
\AmbigramChallengeSecond
\end{{document}}
""")
        if args.subcommand == "ambigram_third":
            f.writelines(
fr"""
\def\AmbiWinnerThird{{{latex_escape(args.winner)}}}
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
                    suggestions_formatted += fr"""\setpollglyph{{{i}}}{{{match_and_format_font(glyph, fonts, None, None, 40, args.verbose)}}}
    """
            else:
                print("No arguments given; taking suggestions from glyph_suggestions.txt...")
                with open("glyph_suggestions.txt", "r") as g:
                    lines = g.readlines()
                for line in lines:
                    a = line.strip().split("\t")[::-1]
                    glyph = (a or [None]).pop()
                    size_override = (a or [None]).pop()
                    font_override = (a or [None]).pop()
                    if glyph:
                        i += 1
                        suggestions_formatted += fr"""\setpollglyph{{{i}}}{{{match_and_format_font(glyph, fonts, font_override, size_override, 40, args.verbose)}}}
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
                    suggestions_formatted += fr"""\setpollambi{{{i}}}{{{match_and_format_font(ambi, fonts, None, None, 28, args.verbose)}}}
    """
            else:
                print("No arguments given; taking suggestions from ambigram_suggestions.txt...")
                with open("ambigram_suggestions.txt", "r") as g:
                    lines = g.readlines()
                for line in lines:
                    a = line.strip().split("\t")[::-1]
                    ambi = (a or [None]).pop()
                    size_override = (a or [None]).pop()
                    font_override = (a or [None]).pop()
                    if ambi:
                        i += 1
                        suggestions_formatted += fr"""\setpollambi{{{i}}}{{{match_and_format_font(ambi, fonts, font_override, size_override, 40, args.verbose)}}}
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
    sys.exit(extract_from_pdf("weekly_challenges.pdf", args.out or args.subcommand, 1000, 50, args.verbose))
