<?php
if (!($row = (rand(0, 10) ? [5] : null))) {
    // do nothing
}
else {
    echo $row[0];
}