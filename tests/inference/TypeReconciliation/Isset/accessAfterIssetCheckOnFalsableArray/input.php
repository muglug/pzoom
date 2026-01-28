<?php
/**
 * @return array{b?: string}|false
 */
function returnPossiblyFalseArray() {
    return rand(0, 1) ? false : (rand(0, 1) ? ["b" => "hello"] : []);
}

function foo() : void {
    $arr = returnPossiblyFalseArray();
    /** @psalm-suppress PossiblyInvalidArrayAccess */
    if (!isset($arr["b"])) {}
    echo $arr["b"];
}
