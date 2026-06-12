<?php
scope($_GET["foo"]);
function scope(array $foo)
{
    $var = $foo + [];
    var_dump($var);
}
