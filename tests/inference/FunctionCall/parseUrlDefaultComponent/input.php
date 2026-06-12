<?php
$component = -1;
$url = "foo";
$a = parse_url($url, -1);
$b = parse_url($url, -42);
$c = parse_url($url, $component);
