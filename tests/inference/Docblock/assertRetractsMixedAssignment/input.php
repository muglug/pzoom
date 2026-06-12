<?php
/** @return mixed */
function givesMixed() { return 1; }

function takesMixed(): void
{
    $x = givesMixed();
    assert(is_string($x));
    echo $x;
}
