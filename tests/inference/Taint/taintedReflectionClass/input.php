<?php
$name = $_GET["name"];
$reflector = new ReflectionClass($name);
$reflector->newInstance();
