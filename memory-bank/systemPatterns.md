# System Patterns

## Architecture Overview

### High-Level Architecture

The application follows a **desktop-native architecture** with clear separation between frontend and backend:

- **Frontend Layer**: React-based UI running in a WebView, handling presentation and user interaction
- **Backend Layer**: Rust-based Tauri core handling file I/O, database operations, and performance-critical tasks
- **Communication Layer**: Tauri IPC bridge enabling type-safe communication between layers
- **Data Layer**: SQLite database for metadata storage with direct filesystem access for images

### Component Relationships

```
User Interface (React)
    ↕ (Tauri IPC)
Business Logic (Rust Commands)
    ↕
Database Layer (SQLite + RwLock)
    ↕
Filesystem (Images via Asset Protocol)
```

## Core Architectural Patterns

### 1. Managed State Injection

**Concept**: Shared resources (like database connections) are injected into the Tauri application at startup and made available to all command handlers without global state.

**Benefits**:

- Type-safe access to shared resources
- Automatic lifetime management
- Testable with mock implementations
- Avoids global variables and their associated problems

### 2. Concurrent Read-Write Pattern

**Concept**: Database access uses read-write locks to allow multiple concurrent readers while ensuring write safety.

**Why This Matters**:

- UI remains responsive during database queries
- Multiple components can read simultaneously
- Writes are serialized to prevent corruption
- Combined with SQLite's WAL mode for even better concurrency

### 3. Asset Protocol for Media Loading

**Concept**: Images are accessed directly from the filesystem using Tauri's asset protocol rather than sending binary data through IPC.

**Key Principle**:

- Backend returns file paths only
- Frontend uses asset protocol to load images directly
- No base64 encoding or large data transfers through IPC
- Prevents "Headers Too Large" errors and improves performance

**Security Consideration**: Asset protocol requires explicit scope configuration to allow file access.

### 4. Absolute Path Storage

**Concept**: Store complete, absolute file paths in the database rather than relative paths.

**Rationale**:

- Application working directory may vary
- Eliminates path resolution errors
- Works reliably across different launch contexts
- Avoid platform-specific path canonicalization issues

### 5. Command-Based IPC

**Concept**: Frontend-backend communication follows a command pattern with type-safe serialization.

**Characteristics**:

- Commands are named functions with defined input/output types
- All data serialized as JSON across IPC boundary
- Error handling through Result types
- Keeps command handlers thin - delegate to business logic

### 6. State Slicing

**Concept**: Frontend state is organized into separate, domain-specific stores rather than a monolithic global state.

**Planned Domains**:

- **Library State**: Image collection and loading
- **Search State**: Query and filtering
- **Tags State**: Tag management and associations
- **UI State**: View preferences and selections
- **Settings State**: User configuration

**Benefits**:

- Components only subscribe to relevant state
- Better performance (fewer re-renders)
- Easier to reason about and test
- Clear separation of concerns

## Key Technical Decisions

### Tauri over Electron

**Decision**: Use Tauri v2 instead of Electron.

**Rationale**:

- 20x smaller bundle size (3-5 MB vs 100+ MB)
- Lower memory footprint
- Rust security guarantees
- Native platform integration
- Better performance for file operations

### Local-First Architecture

**Decision**: All data stored and processed locally, no cloud dependency.

**Implications**:

- Complete user privacy
- Works offline by default
- No subscription or API costs
- Higher performance (no network latency)
- User owns and controls all data

### SQLite with WAL Mode

**Decision**: Use SQLite in Write-Ahead Logging mode.

**Benefits**:

- Proven reliability for local storage
- ACID guarantees prevent corruption
- WAL mode allows concurrent reads and writes
- No separate database server needed
- Perfect for desktop applications

### React with TypeScript

**Decision**: Modern React (v19) with full TypeScript integration.

**Benefits**:

- Type safety across frontend codebase
- Rich ecosystem of components and tools
- Excellent developer experience
- Strong community support
- Good performance with modern React features

## Data Flow Patterns

### Image Loading Flow

1. **Scan Request**: User triggers directory scan in UI
2. **IPC Command**: Frontend invokes Rust command with directory path
3. **Filesystem Scan**: Rust recursively scans directory for image files
4. **Database Insert**: Found images stored in SQLite with absolute paths
5. **Response**: Command returns success/count to frontend
6. **UI Update**: Frontend queries database for image list
7. **Display**: UI renders masonry grid with asset protocol URLs

### Tag Management Flow

1. **User Action**: Select images and add/remove tags in UI
2. **IPC Command**: Send tag operation with image IDs and tag names
3. **Database Update**: Rust updates tag tables and associations
4. **Response**: Return updated image data with new tags
5. **State Sync**: Frontend updates local state
6. **UI Refresh**: Tags displayed immediately on images

### Search/Filter Flow

1. **User Input**: User types query or selects filter tags
2. **Local Filtering**: Frontend can filter in-memory results (fast path)
3. **Database Query**: Complex queries go to backend via IPC
4. **Result Return**: Matching images returned to frontend
5. **Grid Update**: Masonry grid updates to show filtered results
6. **Real-time Feedback**: No explicit search button - updates as user types

## Future Architecture Considerations

### ML/AI Integration Pattern

When implementing semantic search and recommendations:

- **ONNX Runtime** in Rust backend for CLIP model inference
- **Vector Storage** extension to SQLite or separate vector database
- **Batch Processing** for embedding generation (process multiple images at once)
- **Caching Strategy** store embeddings, compute once per image
- **Background Processing** don't block UI during embedding generation

### Performance Optimization Patterns

As collection size grows:

- **Virtual Scrolling** in masonry grid (render only visible images)
- **Lazy Loading** with intersection observer for images
- **Thumbnail Generation** create and cache smaller versions for grid
- **Database Indexing** on frequently queried fields (tags, dates)
- **Pagination** load images in chunks rather than all at once

### Extensibility Patterns

For future feature additions:

- **Plugin Architecture** potential for community extensions
- **Event System** pub/sub for decoupled feature communication
- **Configuration** JSON-based settings for user customization
- **Import/Export** standardized data formats for interoperability

## Error Handling Philosophy

### Backend Errors

- Return `Result` types, never panic in production code
- Provide meaningful error messages to frontend
- Log errors appropriately for debugging
- Graceful degradation when possible

### Frontend Errors

- Display user-friendly error messages
- Provide recovery actions when applicable
- Never crash the application
- Log to console for debugging

### Data Integrity

- Database transactions for multi-step operations
- Validate paths before storage
- Handle missing files gracefully (moved/deleted)
- Regular integrity checks option

## Security Considerations

### File Access

- Asset protocol requires explicit scope configuration
- Validate all user-provided paths
- Prevent directory traversal attacks
- Sandboxed WebView environment

### Database Security

- Use parameterized queries (prevent SQL injection)
- Input validation on all commands
- No user-provided SQL execution
- Regular backups recommended

## Testing Strategy

### Backend Testing

- Unit tests for business logic
- Integration tests for database operations
- Mock filesystem for scanner tests
- Property-based testing for data integrity

### Frontend Testing

- Component unit tests
- Integration tests with mocked IPC
- E2E tests for critical user flows
- Visual regression tests for UI consistency

## Performance Targets

- **Startup Time**: < 2 seconds to first render
- **Scan Speed**: 1000+ images per second
- **Search Latency**: < 100ms for tag-based search
- **Grid Rendering**: 60fps scrolling with visible images
- **Memory Usage**: < 500MB for 10,000 images
- **Database Queries**: < 50ms for typical operations

## Code Organization Principles

### Backend Structure

- **Modules**: Organized by domain (db, filesystem, commands)
- **Separation**: Business logic separate from Tauri handlers
- **Reusability**: Core logic usable outside Tauri context
- **Testing**: Each module independently testable

### Frontend Structure

- **Components**: Atomic, reusable UI components
- **Stores**: Domain-specific state management
- **Hooks**: Shared logic in custom hooks
- **Types**: Centralized type definitions

## Migration and Evolution

### Database Migrations

- Version tracking in database
- Migration scripts for schema changes
- Backward compatibility where possible
- Data validation after migrations

### API Versioning

- IPC command versioning for major changes
- Deprecation warnings before removal
- Compatibility shims when needed
- Clear upgrade paths for users

## Development Workflow Patterns

### Development Mode

- Hot reload for frontend changes
- Fast compilation for Rust changes
- Detailed logging enabled
- Dev-only debugging tools

### Production Mode

- Optimized builds
- Minimal logging
- Error reporting without sensitive data
- Performance monitoring
