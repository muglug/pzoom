<?php
$a = 1;
$b = &$a;
$b = 2;
unset($b);
$b = 3;
