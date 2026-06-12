<?php
$input = strtr('data', $_GET['taint'], 'data');
setcookie($input, 'value');
