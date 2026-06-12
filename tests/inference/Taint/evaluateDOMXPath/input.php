<?php
function evaluateExpression(DOMXPath $xpath) : mixed {
    $expression = $_GET["expression"];
    return $xpath->evaluate($expression);
}
