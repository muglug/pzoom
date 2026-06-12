<?php
function getStr() : ?string {
    return rand(0,1) ? "test" : null;
}

function test() : void {
    $g = getStr();
    /** @psalm-suppress PossiblyNullArgument */
    $x = strtoupper($g);
    $c = "prefix " . (strtoupper($g ?? "") === "x" ? "xa" : "ya");
    echo "$x, $c\n";
}
