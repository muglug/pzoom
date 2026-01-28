<?php
$lilstring = "";

$n = new SimpleXMLElement($lilstring);
$n = $n->b;

if (!$n instanceof SimpleXMLIterator) {
    return;
}

if (!$n) {
    echo "false";
}
