<?php
if (is_string($_SESSION["abc"])) {
    echo substr($_SESSION["abc"], 1, 2);
}
