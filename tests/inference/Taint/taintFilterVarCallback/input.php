<?php
$get = filter_var($_GET, FILTER_CALLBACK, ["options" => "trim"]);

echo $get["test"];
