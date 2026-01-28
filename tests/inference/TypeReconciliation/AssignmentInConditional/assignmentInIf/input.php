<?php
if ($row = (rand(0, 10) ? [5] : null)) {
    echo $row[0];
}