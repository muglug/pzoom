<?php
$mysqli = new mysqli();
echo $mysqli->real_escape_string($_GET["a"]);
