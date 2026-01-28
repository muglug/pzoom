<?php
if (($row = rand(0,10) ? [1] : null) !== null) {
   echo $row[0];
}