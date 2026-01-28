<?php
if (($row = rand(0,10) ? [1] : false) !== false) {
   echo $row[0];
}