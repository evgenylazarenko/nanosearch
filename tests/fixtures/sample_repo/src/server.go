package main

import (
	"fmt"
	"net/http"
	"sync"
	"time"
)

// ServerConfig holds the configuration for the HTTP server.
type ServerConfig struct {
	Host         string
	Port         int
	ReadTimeout  time.Duration
	WriteTimeout time.Duration
}

// Server wraps an HTTP server with graceful shutdown support.
type Server struct {
	config  ServerConfig
	handler http.Handler
	mu      sync.RWMutex
	running bool
}

// NewServer creates a new Server with the given configuration.
func NewServer(config ServerConfig, handler http.Handler) *Server {
	return &Server{
		config:  config,
		handler: handler,
	}
}

// Start begins listening for incoming connections.
func (s *Server) Start() error {
	s.mu.Lock()
	s.running = true
	s.mu.Unlock()

	addr := fmt.Sprintf("%s:%d", s.config.Host, s.config.Port)
	srv := &http.Server{
		Addr:         addr,
		Handler:      s.handler,
		ReadTimeout:  s.config.ReadTimeout,
		WriteTimeout: s.config.WriteTimeout,
	}

	return srv.ListenAndServe()
}

// IsRunning returns whether the server is currently running.
func (s *Server) IsRunning() bool {
	s.mu.RLock()
	defer s.mu.RUnlock()
	return s.running
}

// HealthCheck handles health check requests.
func HealthCheck(w http.ResponseWriter, r *http.Request) {
	w.WriteHeader(http.StatusOK)
	fmt.Fprintf(w, `{"status": "healthy"}`)
}

const DefaultPort = 8080
const MaxRequestSize = 1048576
