<?php
$get = array_map(function($str) { return trim($str);}, $_GET);
echo $get["test"];
