<?php
$get = array_map(function(string $str) : string { return trim($str);}, $_GET);
echo $get["test"];
