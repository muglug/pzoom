<?php
function bar(): void {
    $do_baz = $config["do_it"] ?? false;
    if ($do_baz) {
        baz();
    }
}
