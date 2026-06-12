<?php
/** @psalm-suppress MissingReturnType */
function returnsBool() { return true; }
$a = fn() : bool => /** @var bool */ returnsBool();
