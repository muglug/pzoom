<?php
  /**
   * @param string|callable $middlewareOrPath
   */
  function pipe($middlewareOrPath, ?callable $middleware = null): void {  }

pipe("zzzz", function() : void {});
