<?php
$a = function() : Closure { return function() : string { return "hello"; }; };
$b = $a()();
