<?php
/** @return void */
function takesString(string $str) {}

$bar = rand(0, 1) ? ["foo" => "bar"] : false;

if (isset($bar["foo"])) {
    takesString($bar["foo"]);
}