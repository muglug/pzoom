<?php
$format = random_bytes(10);
$dt = new DateTime;
$formatted = $dt->format($format);
if (false !== $formatted) {}
function takesString(string $s) : void {}
takesString($formatted);
