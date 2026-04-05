Set Shell sh
Set Theme "Chalkboard"
Set Width 1200
Set Height 800
Set Padding 5
Set FontSize 14

Hide
Type "git config --global user.email 'ci@example.com'"
Enter
Type "git config --global user.name 'CI'"
Enter
Show

Sleep 500ms
Type "gitu"
Sleep 1s
Enter
Sleep 2s
Screenshot ../vhs/screenshot.png
Sleep 500ms
