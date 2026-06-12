<?php
function takesInt(int $i) : void {}

if (preg_match("{foo}", "this is foo", $matches, PREG_OFFSET_CAPTURE)) {
    takesInt($matches[0][1]);
}
