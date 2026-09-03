package com.destroystokyo.paper.exception;

/** Base exception for server-side plugin failures. */
public class ServerException extends RuntimeException {
    public ServerException(String message) { super(message); }
    public ServerException(String message, Throwable cause) { super(message, cause); }
    public ServerException(Throwable cause) { super(cause); }
}
