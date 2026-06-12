<?php
$args = $_POST;

$args = (array) $args;

pg_query($connection, "SELECT * FROM tableA where key = " .$args["key"]);

