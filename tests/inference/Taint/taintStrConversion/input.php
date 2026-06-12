<?php
function foo() : void {
    /** @psalm-suppress PossiblyInvalidCast */
    $a = strtoupper(strtolower((string) $_GET["bad"]));
    echo $a;
}
