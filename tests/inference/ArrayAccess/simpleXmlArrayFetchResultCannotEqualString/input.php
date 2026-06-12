<?php
function foo(SimpleXMLElement $s) : void {
    $b = $s["a"];

    if ($b === "hello" || $b === "1") {}
}
