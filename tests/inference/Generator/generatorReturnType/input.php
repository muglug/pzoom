<?php
/** @return Generator<int, stdClass> */
function g():Generator { yield new stdClass; }

$g = g();
