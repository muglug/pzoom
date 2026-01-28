<?php
if (null === ($row = rand(0,10) ? [1] : null)) {

} else {
    echo $row[0];
}