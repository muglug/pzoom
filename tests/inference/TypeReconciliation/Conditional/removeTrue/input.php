<?php
$a = rand(0, 1) ? new stdClass : true;

if ($a === true) {
  exit;
}

function takesStdClass(stdClass $s) : void {}
takesStdClass($a);