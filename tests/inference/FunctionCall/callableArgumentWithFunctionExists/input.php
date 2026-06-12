<?php
if (function_exists('foo')) {
    register_shutdown_function('foo');
}
