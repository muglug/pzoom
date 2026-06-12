<?php
$mysqli = new mysqli();
echo $mysqli->escape_string($_GET["a"]);
