<?php
class A {}
function takesNullableObj(?A &$a): bool { return true; }

$a = null;

if (takesNullableObj($a) === false) {
    return;
} else {}

if ($a) {}
