<?php
set_error_handler(
function() {
    print_r(func_get_args());
    exit(1);
});
