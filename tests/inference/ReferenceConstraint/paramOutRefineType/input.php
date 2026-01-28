<?php
/**
 * @param-out string $s
 */
function addFoo(?string &$s) : void {
    if ($s === null) {
        $s = "hello";
    }
    $s .= "foo";
}

addFoo($a);

echo strlen($a);
