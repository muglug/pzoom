<?php
function queryExpression(DOMXPath $xpath) : mixed {
    $expression = $_GET["expression"];
    return $xpath->query($expression);
}
