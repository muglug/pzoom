<?php
function f(SimpleXMLElement $elt): void {
    foreach ($elt as $item) {
        f($item);
    }
}
