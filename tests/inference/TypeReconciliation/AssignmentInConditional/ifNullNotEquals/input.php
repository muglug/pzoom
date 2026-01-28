<?php
if (null !== ($row = rand(0,10) ? [1] : null)) {
   echo $row[0];
}