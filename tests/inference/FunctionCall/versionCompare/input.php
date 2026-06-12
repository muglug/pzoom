<?php
/** @return "="|"==" */
function getString() : string {
    return rand(0, 1) ? "==" : "=";
}

$a = version_compare("5.0.0", "7.0.0");
$b = version_compare("5.0.0", "7.0.0", "==");
$c = version_compare("5.0.0", "7.0.0", getString());
