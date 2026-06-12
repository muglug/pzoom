<?php
$get = array_map(fn($str) => trim($str), $_GET);
echo $get["test"];
