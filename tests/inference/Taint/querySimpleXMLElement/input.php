<?php
function queryExpression(SimpleXMLElement $xml) : array|false|null {
    $expression = $_GET["expression"];
    return $xml->xpath($expression);
}
