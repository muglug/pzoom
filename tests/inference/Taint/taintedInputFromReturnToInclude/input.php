<?php
$a = (string) $_GET["file"];
$b = "hello" . $a;
include str_replace("a", "b", $b);
