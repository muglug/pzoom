<?php
$fp = tmpfile();

if ($fp) {
    echo "foo", "\n";
} else {
    echo "bar", "\n";
}

echo var_export([$fp, is_resource($fp), !! $fp], true);

fclose($fp);