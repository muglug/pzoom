<?php
$a = ["15" => "a"];
$b = ["15.7" => "a"];
// since PHP 8 this is_numeric but will not be int key
$c = ["15 " => "a"];
$d = ["-15" => "a"];
// see https://github.com/php/php-src/issues/9029#issuecomment-1186226676
$e = ["+15" => "a"];
$f = ["015" => "a"];
$g = ["1e2" => "a"];
$h = ["1_0" => "a"];
                    
