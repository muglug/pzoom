<?php
$lilstring = "";

$n = new SimpleXMLElement($lilstring);
$n = $n->b;

if (!$n instanceof SimpleXMLElement) {
    return;
}

if (!$n) {
    echo "false";
}
