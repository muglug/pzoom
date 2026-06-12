<?php
$name = $_GET["name"];
$function = new ReflectionFunction($name);
$function->invoke();
