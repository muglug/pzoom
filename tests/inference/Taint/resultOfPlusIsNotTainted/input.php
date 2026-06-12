<?php
$input = is_numeric( $_GET["foo"] ) ? $_GET["foo"] : "";
$var = $input + 1;
var_dump($var);
