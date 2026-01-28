<?php
if (rand(0, 10) > 5) {
    echo "hello";
} elseif ($row = (rand(0, 10) ? [5] : null)) {
    echo $row[0];
}