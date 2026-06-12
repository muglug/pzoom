<?php
function provider(): string {
    return '<?php
        $x = 1;
        /** @psalm-check-type $x = lowercase-string */
    ';
}
echo provider();
