<?php
namespace Bar;

/** @psalm-assert-if-true string $myVar */
function isValidString(?string $myVar) : bool {
    return $myVar !== null && $myVar[0] === "a";
}

$myString = rand(0, 1) ? "abacus" : null;

if (isValidString($myString)) {
    echo "Ma chaine " . $myString;
}
