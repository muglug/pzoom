<?php
$a = ["test" => $_GET["name"]];
$get = array_map("trim", $a);
echo $get["test"];
