on run argv
    if (count of argv) is not 2 then error "usage: keynote.applescript INPUT OUTPUT.pdf"
    set inputFile to POSIX file (item 1 of argv)
    set outputFile to POSIX file (item 2 of argv)
    tell application "Keynote"
        set presentationDocument to open inputFile
        export presentationDocument to outputFile as PDF
        close presentationDocument saving no
    end tell
end run
