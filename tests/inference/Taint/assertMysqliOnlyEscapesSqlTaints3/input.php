<?php
$mysqli = new mysqli();
echo mysqli_escape_string($mysqli, $_GET["a"]);
