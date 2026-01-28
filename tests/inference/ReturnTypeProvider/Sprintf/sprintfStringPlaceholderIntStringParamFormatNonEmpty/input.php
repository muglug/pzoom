<?php
$tmp = rand(0, 10) > 5 ? time() : implode("", array()) . "hello";
$val = sprintf("%s", $tmp);
            
