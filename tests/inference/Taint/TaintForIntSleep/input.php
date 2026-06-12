<?php // --taint-analysis
function s(int $id): void
{
    sleep($id);
}
$value = $_GET["value"];
s($value);
